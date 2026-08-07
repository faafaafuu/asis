// Не открывать консольное окно рядом с приложением на Windows в релизной сборке.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sufler_lib::run()
}
