//! ncm-dump-gui：网易云音乐 ncm 转换器（丰富元数据版）

pub mod commands;
pub mod config;
pub mod library;
pub mod metadata;
pub mod ncm;
pub mod netease;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 启动：加载（或首次生成）配置，应用记忆的窗口尺寸
            let cfg = config::load_or_init(None);

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_size(tauri::LogicalSize::new(
                    cfg.ui.window_width,
                    cfg.ui.window_height,
                ));

                // 关闭时记忆窗口尺寸
                let app_handle = app.handle().clone();
                let win_handle = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Destroyed = event {
                        if let Some(state) = app_handle.try_state::<commands::AppState>() {
                            let mut c = state.config.lock().unwrap();
                            if let (Ok(size), Ok(scale)) =
                                (win_handle.inner_size(), win_handle.scale_factor())
                            {
                                let logical = size.to_logical::<u32>(scale);
                                c.ui.window_width = logical.width.max(480);
                                c.ui.window_height = logical.height.max(320);
                            }
                            if let Some(p) = config::config_path() {
                                let snapshot = c.clone();
                                config::save_atomic(&p, &snapshot).ok();
                            }
                        }
                    }
                });
            }

            app.manage(commands::AppState::new(cfg));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::add_paths,
            commands::remember_dir,
            commands::detect_netease_dirs,
            commands::convert
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
