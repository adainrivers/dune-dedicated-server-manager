mod battlegroup;
mod battlegroup_update;
mod component;
mod discovery;
mod logs;
mod management_api;
mod management_service;
mod preflight;
pub(crate) mod shared;
mod status;
mod status_data;
mod status_disk;
mod status_helpers;
mod status_naming;
mod status_pod_age;
mod tunnel;
mod tunnel_helpers;

pub use battlegroup::{
    restart_remote_battlegroup, start_remote_battlegroup, stop_remote_battlegroup,
};
pub use battlegroup_update::update_remote_battlegroup;
pub use component::{remote_component_log_tail, restart_remote_component};
pub use discovery::detect_remote_ubuntu_servers;
pub use logs::{get_logs_folder, record_operation_log};
pub use management_api::{
    ms_cluster, ms_cron_preview, ms_dump_prune_execute, ms_dump_prune_preview, ms_get_config,
    ms_health, ms_history, ms_list_commands, ms_list_logs, ms_list_runs, ms_list_timezones,
    ms_player_location, ms_publish, ms_search_items, ms_search_journey_nodes, ms_search_players,
    ms_search_skill_modules, ms_search_vehicles, ms_search_xp_event_tags, ms_set_config,
    ms_trigger_run, ms_welcome_grant_retry, ms_welcome_grants, ms_welcome_whisper,
};
pub use management_service::{
    install_management_service, management_service_bundled_version, management_service_status,
    restart_management_service, uninstall_management_service,
};
pub use preflight::check_remote_sudo;
pub use status::{remote_server_components, remote_server_status};
pub use tunnel::{
    server_tunnel_status, start_custom_tunnel, start_server_tunnel, stop_all_tunnels,
    stop_server_tunnel,
};
