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

    /// Сколько ждать, пока заговорят, и сколько слушать всего.
    private var idlePause: TimeInterval = 12
    private var listenLimit: TimeInterval = 60
    /// Концовки ответа («вот как-то так») и начала коротких встречных
    /// вопросов: их фраза отдаётся сразу, без долгой паузы на подумать.
    private var endings: [String] = []
    private var openers: [String] = []

    class ListenArgs: Decodable {
        let pause: Double?
        let idle: Double?
        let limit: Double?
        let ends: [String]?
        let asks: [String]?
    }

    override func load(webview: WKWebView) {
        super.load(webview: webview)
        self.webview = webview
        synth.delegate = self
        SystemEditMenu.install(on: webview)
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
        let args = try? invoke.parseArgs(ListenArgs.self)
        phrasePause = min(max(args?.pause ?? SuflerPlugin.endOfPhrase, 0.6), 15)
        idlePause = min(max(args?.idle ?? 12, 3), 30)
        listenLimit = min(max(args?.limit ?? 60, 10), 300)
        endings = (args?.ends ?? []).map(SuflerPlugin.plain).filter { !$0.isEmpty }
        openers = (args?.asks ?? []).map(SuflerPlugin.plain).filter { !$0.isEmpty }
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
        activateAudio(listening: true)
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
                    // Окно показывает речь по ходу — видно, что слышно.
                    self.trigger("hearing", data: ["text": self.heard])
                    // Пока человек говорит — ждём; замолчал — фраза готова.
                    self.silence?.invalidate()
                    self.silence = Timer.scheduledTimer(withTimeInterval: self.pauseAfter(self.heard), repeats: false) { _ in
                        self.finishListening(error: nil)
                    }
                    if result.isFinal { self.finishListening(error: nil) }
                } else if error != nil {
                    self.finishListening(error: nil)
                }
            }
        }
        // Совсем молчат — отдать пустое; одна фраза — не дольше предела.
        silence = Timer.scheduledTimer(withTimeInterval: idlePause, repeats: false) { [weak self] _ in
            self?.finishListening(error: nil)
        }
        cap = Timer.scheduledTimer(withTimeInterval: listenLimit, repeats: false) { [weak self] _ in
            self?.finishListening(error: nil)
        }
    }

    /// Пауза, после которой фраза готова: сказал концовку — секунда, короткий
    /// встречный вопрос — две, иначе — пауза на подумать.
    private func pauseAfter(_ text: String) -> TimeInterval {
        let said = SuflerPlugin.plain(text)
        if endings.contains(where: { said.hasSuffix($0) }) { return min(phrasePause, 1.0) }
        let short = said.split(separator: " ").count <= 14
        if short, openers.contains(where: { said.hasPrefix($0) || said.hasPrefix("а " + $0) }) { return min(phrasePause, 1.8) }
        return phrasePause
    }

    /// Без регистра, «ё» и знаков — для сравнения фраз.
    static func plain(_ text: String) -> String {
        let lowered = text.lowercased().replacingOccurrences(of: "ё", with: "е")
        let kept = lowered.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) ? Character($0) : " " }
        return String(kept).split(separator: " ").joined(separator: " ")
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

    /// Звук голоса. Пока Ноа говорит — режим проигрывания: наушники Bluetooth
    /// играют в полном качестве (A2DP). Режим «микрофон + динамик» держал их в
    /// гарнитурном канале — 16 кГц моно, голос звучал как из телефонной трубки.
    /// Слушает — включается микрофон, и только на это время.
    private func activateAudio(listening: Bool = false) {
        let session = AVAudioSession.sharedInstance()
        if listening {
            try? session.setCategory(.playAndRecord, mode: .spokenAudio, options: [.defaultToSpeaker, .allowBluetooth, .allowBluetoothA2DP, .duckOthers])
        } else {
            try? session.setCategory(.playback, mode: .spokenAudio, options: [.duckOthers])
        }
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
    /// Отвечает, что подменено, — для журнала.
    @objc public func systemMenu(_ invoke: Invoke) {
        let editing = (try? invoke.parseArgs(SystemMenuArgs.self))?.editing ?? false
        DispatchQueue.main.async {
            SystemEditMenu.editing = editing
            invoke.resolve(["hooked": SystemEditMenu.hooked.joined(separator: " ")])
        }
    }
}

/// Системное меню выделения («Скопировать», «Перевести», «Найти»…) в окне
/// приложения выключено: над выделенным текстом окно рисует своё —
/// «Копировать», «Прочитать», «Объяснить». В поле ввода — как обычно.
///
/// Публичного выключателя у WKWebView нет, а собирают меню в разных версиях
/// iOS разные места — глушатся все: какие действия доступны
/// (canPerformAction), из чего строится меню (buildMenu), какое меню
/// показать (делегат UIEditMenuInteraction и UITextInput с iOS 17).
/// Что подменилось — `hooked`, окно пишет это в журнал сервера.
enum SystemEditMenu {
    /// Фокус в поле ввода — меню целиком.
    static var editing = false
    static var hooked: [String] = []
    private static var installed = false

    static func install(on webview: WKWebView) {
        guard !installed else { return }
        installed = true
        let content: AnyClass? = NSClassFromString("WKContentView")
        let own: AnyClass? = object_getClass(webview)
        for cls in [content, own].compactMap({ $0 }) {
            hookCanPerform(cls)
            hookBuildMenu(cls)
        }
        if let content {
            hookMenu(content, "editMenuInteraction:menuForConfiguration:suggestedActions:")
            hookTextMenu(content, "editMenuForTextRange:suggestedActions:")
        }
    }

    private static func swap(_ cls: AnyClass, _ selector: Selector, _ block: Any) -> IMP? {
        guard let method = class_getInstanceMethod(cls, selector) else { return nil }
        let original = method_getImplementation(method)
        let imp = imp_implementationWithBlock(block)
        // Свой метод у класса — подменяется; унаследованный — добавляется классу.
        if !class_addMethod(cls, selector, imp, method_getTypeEncoding(method)) {
            method_setImplementation(method, imp)
        }
        hooked.append("\(NSStringFromClass(cls)).\(NSStringFromSelector(selector))")
        return original
    }

    private static func hookCanPerform(_ cls: AnyClass) {
        let selector = #selector(UIResponder.canPerformAction(_:withSender:))
        typealias Can = @convention(c) (AnyObject, Selector, Selector, AnyObject?) -> Bool
        var original: Can?
        let block: @convention(block) (AnyObject, Selector, AnyObject?) -> Bool = { this, action, sender in
            guard SystemEditMenu.editing, let original else { return false }
            return original(this, selector, action, sender)
        }
        if let imp = swap(cls, selector, block) { original = unsafeBitCast(imp, to: Can.self) }
    }

    private static func hookBuildMenu(_ cls: AnyClass) {
        let selector = #selector(UIResponder.buildMenu(with:))
        typealias Build = @convention(c) (AnyObject, Selector, AnyObject) -> Void
        var original: Build?
        let block: @convention(block) (AnyObject, AnyObject) -> Void = { this, builder in
            original?(this, selector, builder)
            guard !SystemEditMenu.editing, let builder = builder as? UIMenuBuilder, builder.system == .context else { return }
            let menus: [UIMenu.Identifier] = [
                .standardEdit, .replace, .share, .lookup, .learn, .find, .format, .font, .textStyle,
                .spelling, .substitutions, .transformations, .speech, .writingDirection, .alignment,
            ]
            for menu in menus { builder.remove(menu: menu) }
        }
        if let imp = swap(cls, selector, block) { original = unsafeBitCast(imp, to: Build.self) }
    }

    /// Делегат меню выделения WebKit: вне поля ввода — пустое меню, его не показывают.
    private static func hookMenu(_ cls: AnyClass, _ name: String) {
        let selector = NSSelectorFromString(name)
        typealias Menu = @convention(c) (AnyObject, Selector, AnyObject, AnyObject, AnyObject) -> AnyObject?
        var original: Menu?
        let block: @convention(block) (AnyObject, AnyObject, AnyObject, AnyObject) -> AnyObject? = { this, interaction, configuration, actions in
            guard SystemEditMenu.editing, let original else { return UIMenu(children: []) }
            return original(this, selector, interaction, configuration, actions)
        }
        if let imp = swap(cls, selector, block) { original = unsafeBitCast(imp, to: Menu.self) }
    }

    /// Меню выделения текста (UITextInput, iOS 17+).
    private static func hookTextMenu(_ cls: AnyClass, _ name: String) {
        let selector = NSSelectorFromString(name)
        typealias Menu = @convention(c) (AnyObject, Selector, AnyObject, AnyObject) -> AnyObject?
        var original: Menu?
        let block: @convention(block) (AnyObject, AnyObject, AnyObject) -> AnyObject? = { this, range, actions in
            guard SystemEditMenu.editing, let original else { return UIMenu(children: []) }
            return original(this, selector, range, actions)
        }
        if let imp = swap(cls, selector, block) { original = unsafeBitCast(imp, to: Menu.self) }
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
