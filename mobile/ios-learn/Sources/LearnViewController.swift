// Обучение NOAH на iPhone: окно обучения Ноа онлайн во весь экран.
//
// Своего обучения в приложении нет — оно то же, что в браузере и в программе
// на компьютере: уроки, урок с Ноа, карточки, задачи, экзамены, общий
// прогресс. Приложение — рамка: открывает его через российское зеркало
// (прямой путь до noahlab.ru с мобильного интернета режется), держит вход,
// пускает к микрофону, а ссылки наружу — Telegram для входа и прочее —
// отдаёт системе. Обновлять его не нужно: новое приходит с сайта.

import UIKit
import WebKit

final class LearnViewController: UIViewController, WKNavigationDelegate, WKUIDelegate {
    /// Зеркало для России; сайт сам ведёт на него и с прямого адреса.
    private static let home = "https://m.noahlab.ru/app/learning.html"

    /// Свои адреса — открываются здесь; всё прочее уходит в систему.
    private static let ownHosts = ["noahlab.ru", "m.noahlab.ru"]

    private var webView: WKWebView!
    private let failure = UILabel()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground

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
        webView.scrollView.contentInsetAdjustmentBehavior = .automatic
        webView.isOpaque = false
        webView.backgroundColor = .systemBackground
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
        refresh.addTarget(self, action: #selector(reload(_:)), for: .valueChanged)
        webView.scrollView.refreshControl = refresh

        failure.numberOfLines = 0
        failure.textAlignment = .center
        failure.textColor = .secondaryLabel
        failure.isHidden = true
        failure.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(failure)
        NSLayoutConstraint.activate([
            failure.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            failure.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 32),
            failure.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -32),
        ])

        openHome()
    }

    /// Обучение — с меткой времени: CDN зеркала иначе отдал бы страницу из кэша.
    private func openHome() {
        let stamp = Int(Date().timeIntervalSince1970)
        guard let url = URL(string: "\(Self.home)?v=\(stamp)") else { return }
        failure.isHidden = true
        webView.load(URLRequest(url: url))
    }

    @objc private func reload(_ sender: UIRefreshControl) {
        if webView.url == nil || !failure.isHidden {
            openHome()
        } else {
            webView.reload()
        }
        sender.endRefreshing()
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
        if scheme == "http" || scheme == "https" || scheme == "tg" || scheme == "mailto" || scheme == "tel" {
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
    @available(iOS 15.0, *)
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

    // MARK: - Нет связи

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        showFailure(error)
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        showFailure(error)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        failure.isHidden = true
    }

    private func showFailure(_ error: Error) {
        // Отменённая загрузка (ушли на другую страницу) — не ошибка.
        if (error as NSError).code == NSURLErrorCancelled { return }
        failure.text = "Нет связи с Ноа онлайн.\nПотяните экран вниз, чтобы попробовать ещё раз."
        failure.isHidden = false
    }

    /// Вернулись в приложение — например, из Telegram после входа: страница
    /// сама проверит код; если она пустая, открываем обучение заново.
    func appBecameActive() {
        if webView.url == nil { openHome() }
    }
}
