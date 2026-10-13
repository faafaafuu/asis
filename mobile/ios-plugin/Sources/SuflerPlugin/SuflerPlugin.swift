import AVFoundation
import AudioToolbox
import ObjectiveC
import Speech
import Tauri
import UIKit
import WebKit

/// Плагин Tauri для iOS: голос Ноа (синтез и распознавание речи), отступы
/// экрана, сигнал, отклик вибромотором; системное меню выделения в окне
/// выключено — над текстом своё меню (SystemEditMenu).
///
/// Голос — системный: AVSpeechSynthesizer говорит, SFSpeechRecognizer слушает,
/// оба по-русски. Команды — те же, что у плагина Android (SuflerPlugin.kt),
/// их зовёт voice_mobile.rs: speak ждёт конца фразы, listen — конца фразы
/// человека (пауза в речи) и отдаёт текст.
///
/// Ограничение платформы не обходится: дополнить меню выделения в ЧУЖИХ
/// приложениях публичным API нельзя — только в своём окне и через «Поделиться».
class SuflerPlugin: Plugin, AVSpeechSynthesizerDelegate, AVAudioPlayerDelegate {
    private weak var webview: WKWebView?
    private var pending: String?

    // Голос
    private let synth = AVSpeechSynthesizer()
    private var speaking: [Invoke] = []
    // Голос Ноа — фраза с сервера (Silero, как на компьютере), файлом MP3.
    private var player: AVAudioPlayer?
    private var playing: Invoke?

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
    /// Пауза для этого слушания: разговору с Ноа хватает короткой — ответ
    /// приходит быстрее; пересказу урока — длинная (по умолчанию).
    private var phrasePause: TimeInterval = SuflerPlugin.endOfPhrase

    class ListenArgs: Decodable {
        let pause: Double?
    }

    override func load(webview: WKWebView) {
        super.load(webview: webview)
        self.webview = webview
        synth.delegate = self
        SystemEditMenu.install()
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
        phrase.voice = SuflerPlugin.bestRussianVoice()
        // rate программы 1.0 — обычная речь; у iOS обычная — 0.5.
        let rate = Float(args.rate ?? 1.0) * AVSpeechUtteranceDefaultSpeechRate
        phrase.rate = min(max(rate, AVSpeechUtteranceMinimumSpeechRate), AVSpeechUtteranceMaximumSpeechRate)
        speaking.append(invoke)
        synth.speak(phrase)
    }

    /// Лучший русский голос из установленных: «улучшенный» и «высокого
    /// качества» звучат живее голоса по умолчанию. Запасной путь — когда
    /// голос Ноа с сервера не пришёл (нет сети).
    static func bestRussianVoice() -> AVSpeechSynthesisVoice? {
        let russian = AVSpeechSynthesisVoice.speechVoices().filter { $0.language.hasPrefix("ru") }
        let best = russian.max { $0.quality.rawValue < $1.quality.rawValue }
        return best ?? AVSpeechSynthesisVoice(language: "ru-RU")
    }

    class PlayArgs: Decodable {
        let path: String
    }

    /// Проигрывает файл фразы; ответ — когда доиграла (или её перебили).
    @objc public func playAudio(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(PlayArgs.self)
        let url = URL(fileURLWithPath: args.path)
        DispatchQueue.main.async {
            self.finishPlaying()
            self.activateAudio()
            do {
                let player = try AVAudioPlayer(contentsOf: url)
                player.delegate = self
                self.player = player
                self.playing = invoke
                if !player.play() {
                    self.finishPlaying(error: "Фраза не проигралась.")
                }
            } catch {
                invoke.reject("Фраза не проигралась: \(error.localizedDescription)")
            }
        }
    }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        finishPlaying()
    }

    func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        finishPlaying(error: "Фраза не проигралась.")
    }

    private func finishPlaying(error: String? = nil) {
        player?.stop()
        player = nil
        let waiting = playing
        playing = nil
        if let error = error {
            waiting?.reject(error)
        } else {
            waiting?.resolve()
        }
    }

    @objc public func stopSpeaking(_ invoke: Invoke) {
        synth.stopSpeaking(at: .immediate)
        finishSpeaking()
        DispatchQueue.main.async { self.finishPlaying() }
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
        let pause = (try? invoke.parseArgs(ListenArgs.self))?.pause ?? SuflerPlugin.endOfPhrase
        phrasePause = min(max(pause, 0.6), 5)
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
                    self.silence = Timer.scheduledTimer(withTimeInterval: self.phrasePause, repeats: false) { _ in
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

    class HapticArgs: Decodable {
        let style: String?
    }

    /// Отклик вибромотором: касание, меню по удержанию, удаление.
    @objc public func haptic(_ invoke: Invoke) {
        let style = (try? invoke.parseArgs(HapticArgs.self))?.style ?? "light"
        DispatchQueue.main.async {
            switch style {
            case "select":
                UISelectionFeedbackGenerator().selectionChanged()
            case "success":
                UINotificationFeedbackGenerator().notificationOccurred(.success)
            case "warning":
                UINotificationFeedbackGenerator().notificationOccurred(.warning)
            case "error":
                UINotificationFeedbackGenerator().notificationOccurred(.error)
            case "medium":
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            case "heavy":
                UIImpactFeedbackGenerator(style: .heavy).impactOccurred()
            case "rigid":
                UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
            default:
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
            }
            invoke.resolve()
        }
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

    // MARK: - Меню выделения

    class SystemMenuArgs: Decodable {
        let editing: Bool
    }

    /// Окно сообщает, в поле ли ввода фокус: там системное меню нужно целиком.
    @objc public func systemMenu(_ invoke: Invoke) {
        let editing = (try? invoke.parseArgs(SystemMenuArgs.self))?.editing ?? false
        DispatchQueue.main.async {
            SystemEditMenu.editing = editing
            invoke.resolve()
        }
    }
}

/// Системное меню выделения («Скопировать», «Перевести», «Найти»…) в окне
/// приложения выключено: над выделенным текстом окно рисует своё —
/// «Копировать», «Прочитать», «Объяснить». В поле ввода — как обычно; вне
/// поля остаётся только «Вставить». Меню WebKit спрашивает у своего вида
/// (WKContentView), какие действия доступны, — ответ подменяется здесь.
enum SystemEditMenu {
    /// Фокус в поле ввода — меню целиком.
    static var editing = false
    private static var installed = false
    private static let paste: Set<String> = ["paste:", "pasteAndMatchStyle:"]

    static func install() {
        guard !installed, let view = NSClassFromString("WKContentView") else { return }
        let selector = #selector(UIResponder.canPerformAction(_:withSender:))
        guard let method = class_getInstanceMethod(view, selector) else { return }
        installed = true
        typealias Can = @convention(c) (AnyObject, Selector, Selector, AnyObject?) -> Bool
        let original = unsafeBitCast(method_getImplementation(method), to: Can.self)
        let filtered: @convention(block) (AnyObject, Selector, AnyObject?) -> Bool = { this, action, sender in
            let allowed = SystemEditMenu.editing || SystemEditMenu.paste.contains(NSStringFromSelector(action))
            guard allowed else { return false }
            return original(this, selector, action, sender)
        }
        let imp = imp_implementationWithBlock(filtered)
        // Свой метод у WKContentView — подменяется; унаследованный — добавляется классу.
        if !class_addMethod(view, selector, imp, method_getTypeEncoding(method)) {
            method_setImplementation(method, imp)
        }
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
