// Use the bundled globe icon for both the native caption and the taskbar.
pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    let window = app.get_webview_window("main").ok_or("Main window missing")?;
    let icon = app.default_window_icon().ok_or("Application icon missing")?;
    window.set_icon(icon.clone())?;
    Ok(())
}
