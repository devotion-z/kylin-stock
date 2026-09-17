mod attachment;
mod backup;
mod database;
mod inventory;
mod migration;
mod single_instance;

use tauri::{AppHandle, Manager};

#[tauri::command]
fn show_main_window(app: AppHandle) -> Result<(), String> {
    app.get_window("main")
        .ok_or_else(|| "找不到应用主窗口".to_string())?
        .show()
        .map_err(|e| format!("无法显示应用主窗口：{e}"))
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| single_instance::acquire(app).map_err(Into::into))
        .invoke_handler(tauri::generate_handler![
            attachment::add_attachment,
            attachment::delete_attachment,
            attachment::get_attachment_data,
            attachment::list_attachments,
            attachment::open_attachment_external,
            backup::create_database_backup,
            backup::restore_database_backup,
            database::database_execute,
            database::delete_location,
            database::database_select,
            database::merge_materials,
            inventory::stock_in,
            inventory::stock_out,
            inventory::stock_in_batch,
            inventory::stock_out_batch,
            inventory::list_inventory_page,
            inventory::delete_stock_transaction,
            inventory::update_stock_transaction,
            inventory::transfer_stock,
            inventory::delete_inventory_position,
            inventory::scan_document,
            migration::initialize_database_schema,
            show_main_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running KylinStock");
}
