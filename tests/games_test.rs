use sim_relay::games::{select_games, GAMES};
use sim_relay::platform::ProcessScanner;

#[test]
fn select_all_deduplicates_to_thirteen_ports() {
    let groups = select_games(&None, true).unwrap();
    assert_eq!(groups.len(), 13);
}

#[test]
fn select_two_games_same_port_gives_one_group() {
    let ids = Some(vec!["f1-24".to_string(), "f1-23".to_string()]);
    let groups = select_games(&ids, false).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].port, 20777);
    assert!(groups[0].display_name.contains("Codemasters"));
}

#[test]
fn select_single_game_uses_game_display_name() {
    let ids = Some(vec!["wreckfest2".to_string()]);
    let groups = select_games(&ids, false).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].display_name, "Wreckfest 2");
}

#[test]
fn select_unknown_id_returns_error() {
    let ids = Some(vec!["not-a-game".to_string()]);
    assert!(select_games(&ids, false).is_err());
}

#[test]
fn select_mixed_family_port_uses_port_display_name() {
    // Port 25555 has SCS (ETS2/ATS) and Giants (FS22/FS25) — mixed families.
    let ids = Some(vec!["ets2".to_string(), "fs22".to_string()]);
    let groups = select_games(&ids, false).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].display_name, "port 25555");
}

#[test]
fn console_flag_set_for_gt7_port() {
    let ids = Some(vec!["gt7".to_string()]);
    let groups = select_games(&ids, false).unwrap();
    assert_eq!(groups.len(), 1);
    assert!(
        groups[0].console,
        "GT7 port group should be marked console=true"
    );
}

#[test]
fn console_flag_unset_for_pc_games() {
    let ids = Some(vec!["f1-25".to_string()]);
    let groups = select_games(&ids, false).unwrap();
    assert_eq!(groups.len(), 1);
    assert!(
        !groups[0].console,
        "F1 25 port group should be console=false"
    );
}

#[test]
fn process_scanner_returns_false_for_unknown_process() {
    let mut scanner = ProcessScanner::new();
    scanner.refresh();
    // An exe that will never be running in CI or on a dev machine.
    assert!(!scanner.is_running(&["sim_relay_nonexistent_game_xyz.exe"]));
}

#[test]
fn process_scanner_is_running_case_insensitive() {
    let mut scanner = ProcessScanner::new();
    scanner.refresh();
    // Querying with different casing should behave the same (not panic).
    let _ = scanner.is_running(&["SIM_RELAY_NONEXISTENT.EXE"]);
    let _ = scanner.is_running(&["sim_relay_nonexistent.exe"]);
}

#[test]
fn game_ids_are_unique() {
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    for game in GAMES {
        assert!(
            ids.insert(game.id),
            "duplicate game id in registry: {}",
            game.id
        );
    }
}

#[test]
fn all_ports_are_nonzero() {
    for game in GAMES {
        assert!(game.default_port > 0, "port 0 for game '{}'", game.id);
    }
}
