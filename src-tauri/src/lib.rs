mod attachment;
mod backup;
mod database;
mod inventory;
mod migration;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            attachment::add_attachment,
            attachment::delete_attachment,
            attachment::get_attachment_data,
            attachment::list_attachments,
            backup::create_database_backup,
            backup::restore_database_backup,
            database::database_execute,
            database::database_select,
            inventory::stock_in,
            inventory::stock_out,
            migration::initialize_database_schema
        ])
        .run(tauri::generate_context!())
        .expect("error while running KylinStock");
}
