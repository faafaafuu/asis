//! SSH-терминал практики на телефоне.
//!
//! На компьютере практика идёт в местной оболочке (portable-pty), а с неё
//! человек сам заходит на сервер по ssh. На телефоне своей оболочки нет —
//! iOS не даёт приложениям её запускать, — и терминал практики сразу
//! открывается на сервере: программа сама подключается по SSH. Всё прочее в
//! практике то же: Ноа видит вывод, ведёт по шагам, проверяет шаг.
//!
//! Вход — паролем или ключом Ноа: программа заводит свой ключ ed25519, а
//! человек один раз кладёт его открытую часть в ~/.ssh/authorized_keys на
//! сервере. Ключ сервера запоминается при первом входе (как known_hosts):
//! подменённый сервер дальше не пройдёт.

#![cfg_attr(desktop, allow(dead_code))]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{client, ChannelMsg};
use tokio::sync::mpsc;

/// Куда и как входить.
#[derive(Debug, Clone, Default)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// Закрытый ключ Ноа в формате OpenSSH; пусто — только пароль.
    pub key: String,
    /// Отпечаток ключа сервера с прошлого входа; пусто — первый вход.
    pub known: String,
    pub cols: u32,
    pub rows: u32,
}

enum Input {
    Data(Vec<u8>),
    Resize(u32, u32),
    Close,
}

/// Открытый сеанс: в него пишут ввод и размер окна.
pub struct Session {
    input: mpsc::UnboundedSender<Input>,
}

impl Session {
    pub fn write(&self, data: &[u8]) -> bool {
        self.input.send(Input::Data(data.to_vec())).is_ok()
    }

    pub fn resize(&self, cols: u32, rows: u32) {
        let _ = self.input.send(Input::Resize(cols, rows));
    }

    pub fn close(&self) {
        let _ = self.input.send(Input::Close);
    }
}

/// Проверка ключа сервера: первый вход — запомнить, дальше — тот же.
struct Trust {
    known: String,
    seen: Arc<Mutex<String>>,
}

impl client::Handler for Trust {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let print = key.fingerprint(HashAlg::Sha256).to_string();
        *self.seen.lock().unwrap_or_else(|err| err.into_inner()) = print.clone();
        Ok(self.known.is_empty() || self.known == print)
    }
}

/// Новый ключ Ноа: закрытый (OpenSSH) и открытая строка для authorized_keys.
pub fn new_key() -> Result<(String, String), String> {
    let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).map_err(|err| format!("ключ не создался: {err}"))?;
    let private = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|err| format!("ключ не записался: {err}"))?
        .to_string();
    Ok((private, public_line(&key)?))
}

/// Открытая строка ключа — то, что кладут в ~/.ssh/authorized_keys.
pub fn public_of(private: &str) -> Result<String, String> {
    let key = PrivateKey::from_openssh(private).map_err(|err| format!("ключ не читается: {err}"))?;
    public_line(&key)
}

fn public_line(key: &PrivateKey) -> Result<String, String> {
    let line = key.public_key().to_openssh().map_err(|err| err.to_string())?;
    Ok(format!("{line} noa-practice"))
}

/// Подключается и открывает оболочку. `output` получает всё, что пришло с
/// сервера, `closed` — почему сеанс кончился. Отдаёт сеанс и отпечаток ключа
/// сервера — его запоминают для следующего входа.
pub async fn open(
    target: Target,
    output: impl Fn(Vec<u8>) + Send + 'static,
    closed: impl FnOnce(String) + Send + 'static,
) -> Result<(Session, String), String> {
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(20)),
        ..Default::default()
    });
    let seen = Arc::new(Mutex::new(String::new()));
    let handler = Trust { known: target.known.clone(), seen: seen.clone() };
    let connecting = client::connect(config, (target.host.as_str(), target.port), handler);
    let mut handle = match tokio::time::timeout(Duration::from_secs(20), connecting).await {
        Err(_) => return Err("Сервер не ответил за 20 секунд — проверьте адрес и порт.".into()),
        Ok(Err(err)) => {
            let print = seen.lock().unwrap_or_else(|err| err.into_inner()).clone();
            if !target.known.is_empty() && !print.is_empty() && print != target.known {
                return Err(format!(
                    "Ключ сервера изменился с прошлого входа ({print}). Так бывает после переустановки — \
                     тогда забудьте сервер и подключитесь заново. Иначе это может быть подмена."
                ));
            }
            return Err(format!("Не подключилось: {err}"));
        }
        Ok(Ok(handle)) => handle,
    };
    let print = seen.lock().unwrap_or_else(|err| err.into_inner()).clone();

    let mut entered = false;
    if !target.key.trim().is_empty() {
        let key = PrivateKey::from_openssh(&target.key).map_err(|err| format!("ключ Ноа не читается: {err}"))?;
        let auth = handle
            .authenticate_publickey(&target.user, PrivateKeyWithHashAlg::new(Arc::new(key), None))
            .await
            .map_err(|err| format!("Вход ключом не удался: {err}"))?;
        entered = auth.success();
    }
    if !entered && !target.password.is_empty() {
        let auth = handle
            .authenticate_password(&target.user, &target.password)
            .await
            .map_err(|err| format!("Вход паролем не удался: {err}"))?;
        entered = auth.success();
    }
    if !entered {
        return Err(if target.password.is_empty() {
            "Сервер не принял ключ Ноа: добавьте его строку в ~/.ssh/authorized_keys или войдите паролем.".into()
        } else {
            "Сервер не пустил: неверный пользователь или пароль.".into()
        });
    }

    let channel = handle.channel_open_session().await.map_err(|err| format!("сеанс не открылся: {err}"))?;
    channel
        .request_pty(false, "xterm-256color", target.cols.max(20), target.rows.max(5), 0, 0, &[])
        .await
        .map_err(|err| format!("терминал не открылся: {err}"))?;
    channel.request_shell(false).await.map_err(|err| format!("оболочка не запустилась: {err}"))?;
    let (mut read, write) = channel.split();
    let (input, mut queue) = mpsc::unbounded_channel::<Input>();

    // Ввод и размер окна — на сервер. Ручка сеанса живёт здесь: пока задача
    // идёт, соединение открыто.
    tauri::async_runtime::spawn(async move {
        while let Some(next) = queue.recv().await {
            match next {
                Input::Data(data) => {
                    if write.data(&data[..]).await.is_err() {
                        break;
                    }
                }
                Input::Resize(cols, rows) => {
                    let _ = write.window_change(cols.max(20), rows.max(5), 0, 0).await;
                }
                Input::Close => break,
            }
        }
        let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "ru").await;
    });

    // Вывод сервера — окну и Ноа.
    tauri::async_runtime::spawn(async move {
        let reason = loop {
            match read.wait().await {
                Some(ChannelMsg::Data { data }) => output(data.to_vec()),
                Some(ChannelMsg::ExtendedData { data, .. }) => output(data.to_vec()),
                Some(ChannelMsg::ExitStatus { exit_status }) => break format!("Оболочка на сервере закрылась (код {exit_status})."),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break "Соединение с сервером закрыто.".to_string(),
                _ => {}
            }
        };
        closed(reason);
    });

    Ok((Session { input }, print))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trip() {
        let (private, public) = new_key().unwrap();
        assert!(public.starts_with("ssh-ed25519 ") && public.ends_with(" noa-practice"));
        assert_eq!(public_of(&private).unwrap(), public);
    }
}
