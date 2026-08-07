import Tauri
import UIKit
import WebKit

/// Плагин Tauri для iOS (SPEC §9.5).
///
/// Важное ограничение платформы, которое здесь не обходится и не маскируется
/// (SPEC §12.2): дополнить системное меню выделения текста в ЧУЖИХ приложениях
/// (Safari, Заметки и т.д.) публичным API нельзя. Поэтому реальных точек входа две:
///
///   1. Собственный контент приложения — `UIEditMenuInteraction` (iOS 16+) или
///      `UIMenuController` (раньше) добавляет пункт «Объяснить» в меню выделения
///      внутри нашего WKWebView/UITextView.
///   2. Share Extension — «Поделиться» → «Объяснить» из любого приложения.
///      Для пользователя это на один тап длиннее, но работает везде.
class SuflerPlugin: Plugin {

    /// Пункт меню называется так же, как на Android и в дизайн-референсе.
    private static let menuTitle = "Объяснить"

    /// Текст, пришедший из Share Extension до готовности фронтенда.
    private var pending: String?

    override func load(webview: WKWebView) {
        super.load(webview: webview)
        attachEditMenu(to: webview)
        pending = SharedSelectionStore.take()
    }

    // MARK: - Команды для фронтенда

    @objc public func pendingSelection(_ invoke: Invoke) {
        let text = pending ?? SharedSelectionStore.take() ?? ""
        pending = nil
        invoke.resolve(["text": text])
    }

    @objc public func integrationStatus(_ invoke: Invoke) {
        // Статус честный: внутри приложения — полноценно, снаружи — только «Поделиться».
        invoke.resolve([
            "kind": "partial",
            "title": "Внутри приложения — полностью, снаружи — через «Поделиться»",
            "hint": """
                iOS не даёт стороннему приложению добавлять пункты в меню выделения текста \
                в чужих приложениях. В собственном ридере Суфлёра пункт «Объяснить» есть; \
                в остальных приложениях выделите текст и выберите «Поделиться» → «Объяснить».
                """,
        ])
    }

    // MARK: - Меню выделения в собственном контенте

    private func attachEditMenu(to webview: WKWebView) {
        if #available(iOS 16.0, *) {
            let interaction = UIEditMenuInteraction(delegate: self)
            webview.addInteraction(interaction)
        } else {
            // Legacy-путь: пункт добавляется глобально в UIMenuController приложения.
            let item = UIMenuItem(
                title: Self.menuTitle,
                action: #selector(SuflerResponder.suflerExplain(_:))
            )
            UIMenuController.shared.menuItems = [item]
        }
    }

    /// Отправляет выделенный текст фронтенду — дальше открывается тот же попап,
    /// что и на десктопе, с тем же набором состояний.
    fileprivate func emitSelection(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        trigger("selection", data: ["text": trimmed])
    }
}

// MARK: - UIEditMenuInteractionDelegate

@available(iOS 16.0, *)
extension SuflerPlugin: UIEditMenuInteractionDelegate {
    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        let explain = UIAction(title: SuflerPlugin.menuTitle) { [weak self] _ in
            guard let self else { return }
            // Текст выделения в WKWebView достаём через JS: прямого доступа к
            // выделению внутри веб-контента у UIKit нет.
            self.manager?.webview?.evaluateJavaScript("window.getSelection().toString()") { value, _ in
                if let text = value as? String {
                    self.emitSelection(text)
                }
            }
        }
        // Свой пункт добавляем к системным, а не заменяем их: «Копировать» и
        // «Выделить всё» пользователю всё ещё нужны.
        return UIMenu(children: suggestedActions + [explain])
    }
}

/// Цель для legacy-меню (iOS 14–15).
@objc final class SuflerResponder: NSObject {
    @objc func suflerExplain(_ sender: Any?) {}
}

/// Обмен с Share Extension через App Group: расширение живёт в отдельном процессе,
/// общая память — единственный способ передать ему текст.
enum SharedSelectionStore {
    static let suiteName = "group.app.sufler.popup"
    static let key = "pendingSelection"

    static func store(_ text: String) {
        UserDefaults(suiteName: suiteName)?.set(text, forKey: key)
    }

    /// Забирает текст ровно один раз, чтобы попап не открылся повторно при следующем старте.
    static func take() -> String? {
        let defaults = UserDefaults(suiteName: suiteName)
        let value = defaults?.string(forKey: key)
        defaults?.removeObject(forKey: key)
        return value
    }
}
