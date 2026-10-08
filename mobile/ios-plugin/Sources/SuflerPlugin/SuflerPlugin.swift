import AVFoundation
import AudioToolbox
import Speech
import Tauri
import UIKit
import WebKit

/// Плагин Tauri для iOS: голос Ноа (синтез и распознавание речи), отступы
/// экрана, сигнал и пункт «Объяснить» в меню выделения нашего окна.
///
/// Голос — системный: AVSpeechSynthesizer говорит, SFSpeechRecognizer слушает,
/// оба по-русски. Команды — те же, что у плагина Android (SuflerPlugin.kt),
/// их зовёт voice_mobile.rs: speak ждёт конца фразы, listen — конца фразы
/// человека (пауза в речи) и отдаёт текст.
///
/// Ограничение платформы не обходится: дополнить меню выделения в ЧУЖИХ
/// приложениях публичным API нельзя — только в своём окне и через «Поделиться».
class SuflerPlugin: Plugin, AVSpeechSynthesizerDelegate {
    private static let menuTitle = "Объяснить"

    private weak var webview: WKWebView?
    private var pending: String?

    // Голос
    private let synth = AVSpeechSynthesizer()
    private var speaking: [Invoke] = []

    // Распознавание
    private let audio = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var listening: Invoke?
    private var heard = ""
    private var silence: Timer?
    private var cap: Timer?

    /// Пауза, после которой фраза считается сказанной. Пересказ урока —
    /// с паузами на подумать, поэтому не секунда.
    private static let endOfPhrase: TimeInterval = 2.2

    override func load(webview: WKWebView) {
        super.load(webview: webview)
        self.webview = webview
        synth.delegate = self
        attachEditMenu(to: webview)
        pending = SharedSelectionStore.take()
    }

    // MARK: - Выделение

    @objc public func pendingSelection(_ invoke: Invoke) {
        let text = pending ?? SharedSelectionStore.take() ?? ""
        pending = nil
        invoke.resolve(["text": text])
    }

    @objc public func pendingVoice(_ invoke: Invoke) {
        invoke.resolve(["pending": false])
    }

    @objc public func integrationStatus(_ invoke: Invoke) {
        invoke.resolve([
            "kind": "partial",
            "title": "Внутри приложения — полностью, снаружи — через «Поделиться»",
            "hint": "iOS не даёт добавлять пункты в меню выделения чужих приложений. В окнах Ноа пункт «Объяснить» есть; в остальных — «Поделиться» → «Объяснить».",
        ])
    }

    // MARK: - Синтез речи

    class SpeakArgs: Decodable {
        let text: String
        let rate: Double?
    }

    /// Говорит фразу; ответ — когда договорила (или её перебили).
    @objc public func speak(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(SpeakArgs.self)
        let text = args.text.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty {
            invoke.resolve()
            return
        }
        activateAudio()
        let phrase = AVSpeechUtterance(string: text)
        phrase.voice = AVSpeechSynthesisVoice(language: "ru-RU")
        // rate программы 1.0 — обычная речь; у iOS обычная — 0.5.
        let rate = Float(args.rate ?? 1.0) * AVSpeechUtteranceDefaultSpeechRate
        phrase.rate = min(max(rate, AVSpeechUtteranceMinimumSpeechRate), AVSpeechUtteranceMaximumSpeechRate)
        speaking.append(invoke)
        synth.speak(phrase)
    }

    @objc public func stopSpeaking(_ invoke: Invoke) {
        synth.stopSpeaking(at: .immediate)
        finishSpeaking()
        invoke.resolve()
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        if !synthesizer.isSpeaking { finishSpeaking() }
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        finishSpeaking()
    }

    private func finishSpeaking() {
        let waiting = speaking
        speaking.removeAll()
        waiting.forEach { $0.resolve() }
    }

    // MARK: - Распознавание речи

    /// Слушает одну фразу и отдаёт её текстом. Конец — пауза в речи.
    @objc public func listen(_ invoke: Invoke) {
        cancelRecognition(resolve: "")
        listening = invoke
        SFSpeechRecognizer.requestAuthorization { status in
            DispatchQueue.main.async {
                guard status == .authorized else {
                    self.finishListening(error: "Нет доступа к распознаванию речи — разрешите в Настройках.")
                    return
                }
                AVAudioSession.sharedInstance().requestRecordPermission { granted in
                    DispatchQueue.main.async {
                        if granted {
                            self.startRecognition()
                        } else {
                            self.finishListening(error: "Нет доступа к микрофону — разрешите в Настройках.")
                        }
                    }
                }
            }
        }
    }

    @objc public func stopListening(_ invoke: Invoke) {
        finishListening(error: nil)
        invoke.resolve()
    }

    @objc public func cancelListening(_ invoke: Invoke) {
        cancelRecognition(resolve: "")
        invoke.resolve()
    }

    private func startRecognition() {
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "ru-RU")), recognizer.isAvailable else {
            finishListening(error: "Распознавание речи сейчас недоступно.")
            return
        }
        activateAudio()
        heard = ""
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        self.request = request

        let input = audio.inputNode
        let format = input.outputFormat(forBus: 0)
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }
        audio.prepare()
        do {
            try audio.start()
        } catch {
            finishListening(error: "Микрофон не включился: \(error.localizedDescription)")
            return
        }

        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let result {
                    self.heard = result.bestTranscription.formattedString
                    // Пока человек говорит — ждём; замолчал — фраза готова.
                    self.silence?.invalidate()
                    self.silence = Timer.scheduledTimer(withTimeInterval: Self.endOfPhrase, repeats: false) { _ in
                        self.finishListening(error: nil)
                    }
                    if result.isFinal { self.finishListening(error: nil) }
                } else if error != nil {
                    self.finishListening(error: nil)
                }
            }
        }
        // Совсем молчат — через 12 с отдать пустое; одна фраза — не дольше минуты.
        silence = Timer.scheduledTimer(withTimeInterval: 12, repeats: false) { [weak self] _ in
            self?.finishListening(error: nil)
        }
        cap = Timer.scheduledTimer(withTimeInterval: 60, repeats: false) { [weak self] _ in
            self?.finishListening(error: nil)
        }
    }

    private func finishListening(error: String?) {
        let invoke = listening
        let text = heard
        cancelRecognition(resolve: nil)
        guard let invoke else { return }
        if let error, text.isEmpty {
            invoke.reject(error)
        } else {
            invoke.resolve(["text": text])
        }
    }

    /// Остановить микрофон; `resolve` — чем ответить ждущему listen.
    private func cancelRecognition(resolve: String?) {
        silence?.invalidate()
        cap?.invalidate()
        silence = nil
        cap = nil
        if audio.isRunning {
            audio.stop()
            audio.inputNode.removeTap(onBus: 0)
        }
        request?.endAudio()
        task?.cancel()
        request = nil
        task = nil
        if let resolve, let invoke = listening {
            invoke.resolve(["text": resolve])
        }
        if resolve != nil || listening != nil { listening = nil }
    }

    private func activateAudio() {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playAndRecord, mode: .spokenAudio, options: [.defaultToSpeaker, .allowBluetooth, .duckOthers])
        try? session.setActive(true, options: .notifyOthersOnDeactivation)
    }

    // MARK: - Прочее

    @objc public func chime(_ invoke: Invoke) {
        AudioServicesPlaySystemSound(1007)
        invoke.resolve()
    }

    /// Отступы под вырез, строку состояния и полоску «домой».
    @objc public func insets(_ invoke: Invoke) {
        DispatchQueue.main.async {
            let window = UIApplication.shared.connectedScenes
                .compactMap { ($0 as? UIWindowScene)?.windows.first { $0.isKeyWindow } }
                .first
            let safe = window?.safeAreaInsets ?? .zero
            invoke.resolve([
                "top": Double(safe.top),
                "bottom": Double(safe.bottom),
                "left": Double(safe.left),
                "right": Double(safe.right),
                "keyboard": false,
            ])
        }
    }

    @objc public func barStyle(_ invoke: Invoke) {
        invoke.resolve()
    }

    class UrlArgs: Decodable {
        let url: String
    }

    @objc public func openUrl(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(UrlArgs.self)
        DispatchQueue.main.async {
            if let url = URL(string: args.url) { UIApplication.shared.open(url) }
            invoke.resolve()
        }
    }

    @objc public func clipboard(_ invoke: Invoke) {
        DispatchQueue.main.async {
            invoke.resolve(["text": UIPasteboard.general.string ?? ""])
        }
    }

    // MARK: - Меню выделения в своём окне

    private func attachEditMenu(to webview: WKWebView) {
        if #available(iOS 16.0, *) {
            webview.addInteraction(UIEditMenuInteraction(delegate: self))
        }
    }

    fileprivate func emitSelection(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        trigger("selection", data: ["text": trimmed])
    }
}

@available(iOS 16.0, *)
extension SuflerPlugin: UIEditMenuInteractionDelegate {
    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        let explain = UIAction(title: SuflerPlugin.menuTitle) { [weak self] _ in
            self?.webview?.evaluateJavaScript("window.getSelection().toString()") { value, _ in
                if let text = value as? String { self?.emitSelection(text) }
            }
        }
        return UIMenu(children: suggestedActions + [explain])
    }
}

/// Обмен с расширением «Поделиться» через App Group.
enum SharedSelectionStore {
    static let suiteName = "group.app.sufler.popup"
    static let key = "pendingSelection"

    static func store(_ text: String) {
        UserDefaults(suiteName: suiteName)?.set(text, forKey: key)
    }

    static func take() -> String? {
        let defaults = UserDefaults(suiteName: suiteName)
        let value = defaults?.string(forKey: key)
        defaults?.removeObject(forKey: key)
        return value
    }
}

/// Точка входа: её зовёт Rust (`ios_plugin_binding!(init_plugin_sufler)`).
@_cdecl("init_plugin_sufler")
func initPlugin() -> Plugin {
    return SuflerPlugin()
}
