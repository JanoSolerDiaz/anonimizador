// Evita la ventana de consola adicional en Windows en release. No quitar.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    anonimizador_lib::run()
}
