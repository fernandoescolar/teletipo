fn main() {
    let update_rx = app_cli::updater::spawn_update();
    app_cli::run(update_rx);
}
