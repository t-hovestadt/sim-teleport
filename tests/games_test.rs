use sim_relay::games::select_games;

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
