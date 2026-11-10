mod comandos;
mod conector;
mod discord;
mod geo;
mod nucleo;
mod pac;
mod piscina;
mod proxy_local;
mod rotas;
mod saidas;
mod sessao;
mod teste;
mod vigia;

use nucleo::Nucleo;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

fn mostrar(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();

    // Chamado pelo desinstalador: devolve o proxy automático e sai.
    if args.iter().any(|a| a == "--limpar") {
        let _ = pac::desativar();
        return;
    }
    let com_o_pc = args.iter().any(|a| a == "--bandeja");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| mostrar(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--bandeja"]),
        ))
        .setup(move |app| {
            let pasta = app.path().app_config_dir()?;
            std::fs::create_dir_all(&pasta)?;
            let n = Arc::new(Nucleo::novo(pasta, app.handle().clone()));
            app.manage(n.clone());
            tauri::async_runtime::spawn(nucleo::iniciar(n, com_o_pc));

            let abrir = MenuItem::with_id(app, "abrir", "Abrir Live Ponte", true, None::<&str>)?;
            let reiniciar = MenuItem::with_id(app, "reiniciar", "Reiniciar Discord pela saída", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&abrir, &reiniciar, &sep, &sair])?;

            TrayIconBuilder::with_id("principal")
                .icon(app.default_window_icon().cloned().expect("ícone do app"))
                .tooltip("Live Ponte")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "abrir" => mostrar(app),
                    "reiniciar" => {
                        if let Some(n) = app.try_state::<Arc<Nucleo>>() {
                            let n = n.inner().clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = n.reiniciar_discord().await;
                            });
                        }
                    }
                    "sair" => {
                        if let Some(n) = app.try_state::<Arc<Nucleo>>() {
                            n.encerrar();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        mostrar(tray.app_handle());
                    }
                })
                .build(app)?;

            if !com_o_pc {
                mostrar(app.handle());
            }
            Ok(())
        })
        .on_window_event(|janela, ev| {
            // Fechar a janela só a esconde: o programa continua na bandeja.
            if let WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = janela.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            comandos::estado,
            comandos::adicionar_saida,
            comandos::renomear_saida,
            comandos::remover_saida,
            comandos::escolher_saida,
            comandos::testar_saida,
            comandos::testar_direto,
            comandos::atualizar_piscina,
            comandos::usar_proxy_publico,
            comandos::abrir_github,
            comandos::reiniciar_discord,
            comandos::pausar,
            comandos::definir_opcoes,
            comandos::exportar_convite,
            comandos::importar_convite,
            comandos::autostart_ativo,
            comandos::definir_autostart,
            comandos::sair,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Live Ponte");

    app.run(|app, ev| {
        if let RunEvent::Exit = ev {
            if let Some(n) = app.try_state::<Arc<Nucleo>>() {
                n.encerrar();
            }
        }
    });
}
