import MobileCoreServices
import UIKit
import UniformTypeIdentifiers

/// Share Extension «Объяснить» (SPEC §9.5, §12.2).
///
/// Единственный способ дотянуться до выделения в чужих приложениях на iOS:
/// пользователь выделяет текст → «Поделиться» → «Объяснить». Расширение забирает
/// текст, кладёт его в общий с приложением контейнер и открывает приложение,
/// которое показывает тот же попап.
final class ActionViewController: UIViewController {

    override func viewDidLoad() {
        super.viewDidLoad()
        // Своего интерфейса у расширения нет: оно должно отработать незаметно.
        view.backgroundColor = .clear
        extractText()
    }

    private func extractText() {
        guard let item = extensionContext?.inputItems.first as? NSExtensionItem,
              let provider = item.attachments?.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) })
        else {
            finish()
            return
        }

        provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) { [weak self] value, _ in
            let text = (value as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !text.isEmpty {
                // Тот же App Group, что читает плагин.
                UserDefaults(suiteName: "group.app.sufler.popup")?
                    .set(text, forKey: "pendingSelection")
            }
            DispatchQueue.main.async {
                self?.openHostApp()
                self?.finish()
            }
        }
    }

    /// Открытие основного приложения по собственной URL-схеме. Расширению нельзя
    /// вызывать UIApplication.shared напрямую, поэтому идём через responder chain.
    private func openHostApp() {
        guard let url = URL(string: "sufler://explain") else { return }
        var responder: UIResponder? = self
        while let current = responder {
            if let application = current as? UIApplication {
                application.open(url, options: [:], completionHandler: nil)
                return
            }
            responder = current.next
        }
    }

    private func finish() {
        extensionContext?.completeRequest(returningItems: nil, completionHandler: nil)
    }
}
