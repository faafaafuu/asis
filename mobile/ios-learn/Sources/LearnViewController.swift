// Обучение NOAH на iPhone: Ноа онлайн во весь экран.
//
// Своего обучения в приложении нет — оно то же, что в браузере и в программе
// на компьютере: уроки, урок с Ноа, карточки, задачи, экзамены, общий
// прогресс. Приложение — рамка: открывает Ноа онлайн через российское зеркало
// (прямой путь до noahlab.ru с мобильного интернета режется), держит вход,
// пускает к микрофону, а ссылки наружу — Telegram для входа и прочее —
// отдаёт системе. Обновлять его не нужно: новое приходит с сайта.
//
// Открывается главная Ноа онлайн, а не сама страница урока: на ней вход и
// карточка «Обучение». Страница урока без входа пустая — на тёмной теме
// iPhone это был просто чёрный экран.

import UIKit
import WebKit

final class LearnViewController: UIViewController, WKNavigationDelegate, WKUIDelegate {
    /// Сначала зеркало для России, не ответило — прямой адрес.
    private static let homes = ["https://m.noahlab.ru/app/", "https://noahlab.ru/app/"]

    /// Свои адреса — открываются здесь; всё прочее уходит в систему.
    private static let ownHosts = ["noahlab.ru", "m.noahlab.ru"]

    /// Сколько ждать первую страницу, прежде чем пробовать другой адрес.
    private static let patience: TimeInterval = 20

    private var webView: WKWebView!
    private let status = UILabel()
    private let spinner = UIActivityIndicatorView(style: .large)
    private var homeIndex = 0
    private var loadedOnce = false
    private var watchdog: Timer?

    override func viewDidLoad() {
        super.viewDidLoad()
        // Сайт светлый; на тёмной теме iPhone пустое окно было бы чёрным.
        overrideUserInterfaceStyle = .light
        view.backgroundColor = .white

        let config = WKWebViewConfiguration()
        // Вход и прогресс живут в cookie и хранилище сайта — между запусками тоже.
        config.websiteDataStore = .default()
        // Голос Ноа звучит сам, без отдельного касания на каждую фразу.
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        config.preferences.javaScriptCanOpenWindowsAutomatically = true

        webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.allowsBackForwardNavigationGestures = true
        webView.backgroundColor = .white
        webView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(webView)
        NSLayoutConstraint.activate([
            webView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            webView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            webView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])

        // Потянуть вниз — обновить: так же, как в браузере.
        let refresh = UIRefreshControl()
        refresh.addTarget(self, action: #selector(pulled(_:)), for: .valueChanged)
        webView.scrollView.refreshControl = refresh

        // Пока первая страница не пришла — видно, что идёт загрузка, а не пустота.
        spinner.color = .gray
        spinner.translatesAutoresizingMaskIntoConstraints = false
        status.numberOfLines = 0
        status.textAlignment = .center
        status.textColor = .darkGray
        status.font = .preferredFont(forTextStyle: .body)
        status.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(spinner)
        view.addSubview(status)
        NSLayoutConstraint.activate([
            spinner.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            spinner.centerYAnchor.constraint(equalTo: view.centerYAnchor, constant: -30),
            status.topAnchor.constraint(equalTo: spinner.bottomAnchor, constant: 16),
            status.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 32),
            status.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -32),
        ])

        // Касание по сообщению об ошибке — попробовать ещё раз.
        status.isUserInteractionEnabled = true
        status.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(retry)))

        openHome()
    }

    /// Главная — с меткой времени: CDN зеркала иначе отдал бы её из кэша.
    private func openHome() {
        let stamp = Int(Date().timeIntervalSince1970)
        guard let url = URL(string: "\(Self.homes[homeIndex])?v=\(stamp)") else { return }
        show("Загружаю Ноа онлайн…", busy: true)
        webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: Self.patience))
        watchdog?.invalidate()
        watchdog = Timer.scheduledTimer(withTimeInterval: Self.patience, repeats: false) { [weak self] _ in
            self?.giveUp("Сайт долго не отвечает.")
        }
    }

    private func show(_ text: String?, busy: Bool) {
        status.text = text
        status.isHidden = text == nil
        if busy { spinner.startAnimating() } else { spinner.stopAnimating() }
    }

    /// Первый адрес не ответил — следующий; не ответили все — сказать почему.
    private func giveUp(_ reason: String) {
        watchdog?.invalidate()
        guard !loadedOnce else { return show(nil, busy: false) }
        if homeIndex + 1 < Self.homes.count {
            homeIndex += 1
            openHome()
            return
        }
        webView.stopLoading()
        show("Нет связи с Ноа онлайн.\n\(reason)\n\nКоснитесь здесь или потяните экран вниз, чтобы попробовать ещё раз.", busy: false)
    }

    @objc private func retry() {
        guard !loadedOnce else { return }
        homeIndex = 0
        openHome()
    }

    @objc private func pulled(_ sender: UIRefreshControl) {
        sender.endRefreshing()
        if loadedOnce {
            webView.reload()
        } else {
            retry()
        }
    }

    private func isOwn(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return Self.ownHosts.contains { host == $0 || host.hasSuffix(".\($0)") }
    }

    // MARK: - Куда можно

    func webView(
        _ webView: WKWebView,
        decidePolicyFor action: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let url = action.request.url else { return decisionHandler(.cancel) }
        let scheme = url.scheme?.lowercased() ?? ""
        if scheme == "about" || scheme == "blob" || scheme == "data" || isOwn(url) {
            return decisionHandler(.allow)
        }
        // Telegram (вход по коду), почта, чужие сайты — системе. Вход через
        // Telegram сам завершится, когда вы вернётесь: страница ждёт кода.
        if ["http", "https", "tg", "mailto", "tel"].contains(scheme) {
            UIApplication.shared.open(url)
        }
        decisionHandler(.cancel)
    }

    /// Ссылки «в новом окне» — в этом же: окон у приложения нет.
    func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for action: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        if let url = action.request.url {
            if isOwn(url) {
                webView.load(action.request)
            } else {
                UIApplication.shared.open(url)
            }
        }
        return nil
    }

    /// Микрофон — для урока с Ноа голосом. Спрашивает система один раз.
    func webView(
        _ webView: WKWebView,
        requestMediaCapturePermissionFor origin: WKSecurityOrigin,
        initiatedByFrame frame: WKFrameInfo,
        type: WKMediaCaptureType,
        decisionHandler: @escaping (WKPermissionDecision) -> Void
    ) {
        let own = Self.ownHosts.contains { origin.host == $0 || origin.host.hasSuffix(".\($0)") }
        decisionHandler(own ? .grant : .deny)
    }

    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String,
                 initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping () -> Void) {
        let alert = UIAlertController(title: nil, message: message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "OK", style: .default) { _ in completionHandler() })
        present(alert, animated: true)
    }

    func webView(_ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String,
                 initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping (Bool) -> Void) {
        let alert = UIAlertController(title: nil, message: message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "Отмена", style: .cancel) { _ in completionHandler(false) })
        alert.addAction(UIAlertAction(title: "OK", style: .default) { _ in completionHandler(true) })
        present(alert, animated: true)
    }

    // MARK: - Загрузка

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        watchdog?.invalidate()
        loadedOnce = true
        show(nil, busy: false)
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        failed(error)
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        failed(error)
    }

    /// Ответ сервера с ошибкой (502, 404) — тоже повод попробовать другой адрес.
    func webView(
        _ webView: WKWebView,
        decidePolicyFor response: WKNavigationResponse,
        decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void
    ) {
        if !loadedOnce, response.isForMainFrame,
           let http = response.response as? HTTPURLResponse, http.statusCode >= 400 {
            decisionHandler(.cancel)
            giveUp("Сайт ответил ошибкой \(http.statusCode).")
            return
        }
        decisionHandler(.allow)
    }

    private func failed(_ error: Error) {
        let code = (error as NSError).code
        // Отменённая загрузка (ушли на другую страницу, отдали ссылку системе) — не ошибка.
        if code == NSURLErrorCancelled || code == 102 { return }
        if loadedOnce { return }
        giveUp(error.localizedDescription)
    }

    /// Вернулись в приложение — например, из Telegram после входа: страница
    /// сама проверит код. Если она так и не загрузилась — пробуем снова.
    func appBecameActive() {
        if !loadedOnce && spinner.isHidden == false { return }
        if !loadedOnce { retry() }
    }
}
