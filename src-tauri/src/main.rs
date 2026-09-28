#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod atomic;
mod collections;
mod commands;
mod config;
mod core;
mod diag;
mod error;
mod http;
// Живые проверки: настоящий каталог, настоящее ядро, настоящая сеть. В сборку не входят.
#[cfg(test)]
mod live;
mod nodes;
mod paths;
mod render;
mod stamp;
mod system;
mod yaml;

use tauri::Manager;

use app::state::AppState;
use app::tray;

fn main() {
    // Клиент должен работать с правами, а работает без них — поднимаем себя задачей
    // планировщика и уходим (D-087). Самым первым делом: вторая копия появится через
    // мгновение, и успей эта объявиться плагином одиночного запуска, новая просто
    // показала бы ей окно и умерла — то есть повышения так и не случилось бы.
    if system::task::handoff() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app::updates::Updates::default())
        // Вторая копия не поднимается: она показывает окно уже работающей. Без этого
        // трей ломает обычный сценарий — «закрыл окно, запустил ярлык снова» (D-046).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show(app);
        }))
        .setup(|app| {
            // Порядок шагов запуска — список, а не порядок строк здесь (D-101).
            app::boot::run(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Крестик прячет окно, а не выключает VPN (D-046). Выход — из меню трея,
            // и он по-прежнему гасит ядро.
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // Свернули или развернули обратно — ниже решится, видно ли страницу.
                // Разворот из панели задач приходит не всегда размером, но всегда фокусом.
                tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Focused(_) => {}
                _ => return,
            }
            // Спрятанное в трей и свёрнутое окно страница сама не замечает
            // (`system::webview`); показ из трея включает её обратно в `tray::show`.
            let shown =
                window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false);
            if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                system::webview::set_visible(&webview, shown);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::core::core_status,
            commands::core::core_logs,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::client::client_geo_get,
            commands::client::client_geo_set,
            commands::client::client_ping_get,
            commands::client::client_ping_set,
            commands::client::client_mask_get,
            commands::client::client_mask_set,
            commands::client::client_health_get,
            commands::client::client_health_set,
            commands::diag::diag_tools,
            commands::diag::diag_run,
            commands::diag::diag_apply,
            commands::diag::diag_providers,
            commands::advanced::advanced_get,
            commands::advanced::advanced_set,
            commands::config::config_list,
            commands::config::config_read,
            commands::config::config_write,
            commands::config::config_reset,
            commands::mode::mode_set,
            commands::config::config_assembled,
            commands::groups::groups_parse,
            commands::groups::groups_render,
            commands::rulesets::rulesets_list,
            commands::rulesets::rulesets_read,
            commands::rulesets::rulesets_create,
            commands::rulesets::rulesets_delete,
            commands::udp::udp_get,
            commands::udp::udp_set,
            commands::rulesets::rulesets_write,
            commands::rulesets::rulesets_set,
            commands::rules::rules_parse,
            commands::rules::rules_render,
            commands::core::core_install,
            commands::sources::sources_list,
            commands::sources::sources_add,
            commands::sources::sources_add_proxy,
            commands::sources::sources_add_proxy_text,
            commands::sources::sources_proxy_yaml,
            commands::sources::sources_add_file,
            commands::sources::sources_refresh,
            commands::sources::sources_refresh_all,
            commands::sources::sources_delete,
            commands::sources::sources_read,
            commands::sources::sources_write,
            commands::nodes::nodes_list,
            commands::nodes::nodes_ping,
            commands::direction::direction_set,
            commands::connection::connection_snapshot,
            commands::presets::presets_list,
            commands::presets::presets_create,
            commands::presets::presets_select,
            commands::presets::presets_rename,
            commands::presets::presets_delete,
            commands::nodes::nodes_code,
            commands::nodes::nodes_code_set,
            commands::nodes::nodes_entry_set,
            commands::nodes::nodes_delete,
            commands::nodes::nodes_reset,
            commands::system::system_relaunch_elevated,
            commands::system::system_kill_switch_set,
            commands::system::system_autostart_set,
            commands::system::system_always_admin_set,
            commands::system::system_reset,
            commands::system::system_device,
            commands::system::system_language,
            commands::updates::updates_check,
            commands::updates::updates_install,
            commands::core::core_start,
            commands::core::core_stop,
            commands::qd::qd_status,
            commands::qd::qd_call,
            commands::qd::qd_start,
            commands::qd::qd_stop,
            commands::qd::qd_install,
            commands::qd::qd_rules_export,
            commands::qd::qd_rules_import,
            commands::qd::qd_logs,
            commands::core::core_restart,
            commands::core::core_traffic,
            commands::core::core_flush_fake_ip
        ])
        .build(app::boot::context())
        .expect("не удалось собрать приложение")
        .run(|app, event| {
            // Гасим ядро сами и снимаем системный прокси. Клетка (D-058) прибила бы его
            // и так, но штатный выход обязан закрывать процесс, а не расстреливать.
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                // Выход приходит на главный поток, а фаза остановки асинхронна (D-101):
                // ждём её здесь, иначе процесс уйдёт раньше, чем ядро погашено.
                tauri::async_runtime::block_on(app::connect::stop(app, &state));
            }
        });
}
