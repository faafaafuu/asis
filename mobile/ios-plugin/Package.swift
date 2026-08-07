// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "sufler-plugin",
    platforms: [
        // UIEditMenuInteraction — iOS 16+. Ниже работает legacy-путь через
        // UIMenuController, поэтому минимальная версия остаётся 14 (SPEC §9.5).
        .iOS(.v14)
    ],
    products: [
        .library(name: "sufler-plugin", type: .static, targets: ["SuflerPlugin"])
    ],
    dependencies: [
        // Путь подставляется генератором `tauri ios init`.
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "SuflerPlugin",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources/SuflerPlugin"
        )
    ]
)
