mod commands;
mod db;
mod mail;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      db::initialize(app.handle()).map_err(std::io::Error::other)?;
      #[cfg(windows)]
      if let Some(window) = app.get_webview_window("main") {
        let icon = tauri::image::Image::from_app_icon_resource(32)
          .map_err(std::io::Error::other)?;
        window.set_icon(icon).map_err(std::io::Error::other)?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::get_snapshot,
      commands::create_group,
      commands::rename_group,
      commands::delete_group,
      commands::move_account,
      commands::connect_manual_account,
      commands::sync_account,
      commands::load_message_body,
      commands::save_attachment,
      commands::set_message_flags,
      commands::send_plain_text
    ])
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
