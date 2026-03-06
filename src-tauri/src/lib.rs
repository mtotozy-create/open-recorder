mod commands;
mod models;
mod providers;
mod state;
mod storage;

use commands::{
    job::job_get,
    recorder::{
        recorder_export, recorder_pause, recorder_resume, recorder_start, recorder_status,
        recorder_stop,
    },
    session::{session_get, session_list, session_rename},
    settings::{settings_get, settings_update},
    summary::summary_enqueue,
    transcribe::transcribe_enqueue,
};
use state::AppState;

pub fn run() {
    let app_state = AppState::load().expect("failed to initialize persisted app state");

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            recorder_start,
            recorder_pause,
            recorder_resume,
            recorder_stop,
            recorder_status,
            recorder_export,
            session_list,
            session_get,
            session_rename,
            transcribe_enqueue,
            summary_enqueue,
            job_get,
            settings_get,
            settings_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running open-recorder");
}
