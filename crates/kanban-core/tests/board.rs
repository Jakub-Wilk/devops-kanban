//! Board building, move planning and hierarchy against fixtures captured from the media-press
//! org (IT Finance / IT Finance Team), with people anonymised.

use std::collections::HashMap;

use ado_client::{BacklogConfigurationDto, BoardDto, CardSettingsDto, ListResponse, WorkItemDto};
use kanban_core::card::fields_to_fetch;
use kanban_core::workspace::levels_of;
use kanban_core::{
    Board, Card, CardId, Hierarchy, MoveError, MovePlan, MoveTarget, StateCategory, StateMap,
};
use serde_json::json;

const ORDER: &str = "Microsoft.VSTS.Common.StackRank";

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn cards() -> HashMap<CardId, Card> {
    let batch: ListResponse<WorkItemDto> = fixture("workitems_batch.json");
    batch
        .value
        .iter()
        .map(|w| (w.id, Card::from_work_item(w, ORDER)))
        .collect()
}

fn stories_board() -> Board {
    let dto: BoardDto = fixture("board_stories.json");
    let mut board = Board::from_dto("IT Finance", "IT Finance Team", &dto);
    board.place(cards().values());
    board
}

fn target(board: &Board, column: &str) -> MoveTarget {
    MoveTarget {
        column_id: board
            .columns
            .iter()
            .find(|c| c.name == column)
            .unwrap()
            .id
            .clone(),
        done: false,
        lane_id: None,
        previous_id: None,
        next_id: None,
    }
}

#[test]
fn board_reads_columns_and_wef_fields() {
    let board = stories_board();
    assert_eq!(board.columns.len(), 9);
    assert_eq!(board.columns[0].name, "New");
    assert_eq!(board.columns[1].item_limit, 5);
    assert_eq!(
        board.fields.column,
        "WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column"
    );
    assert_eq!(
        board.fields.done,
        "WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column.Done"
    );
    assert_eq!(board.work_item_types, vec!["User Story"]);
    assert!(!board.has_lanes());
}

#[test]
fn placement_uses_board_column_and_skips_other_types() {
    let board = stories_board();
    let col =
        |name: &str| &board.placement[&board.columns.iter().find(|c| c.name == name).unwrap().id];
    assert_eq!(col("Paused"), &vec![111354]);
    assert_eq!(col("Done"), &vec![118258]);
    assert_eq!(col("Active"), &vec![118429]); // the Epic 118254 is not on the Stories board
}

#[test]
fn card_fields_parse() {
    let cards = cards();
    let c = &cards[&111354];
    assert_eq!(c.rev, 11);
    assert_eq!(c.priority, Some(3));
    assert_eq!(c.parent, Some(118181));
    assert_eq!(c.tags, vec!["jira:P1-125"]);
    assert_eq!(c.area_leaf(), "Penny");
    assert_eq!(c.assigned_to.as_ref().unwrap().initials(), "AL");
    assert_eq!(
        cards[&118429].target_date.unwrap().to_string(),
        "2026-11-20"
    );
}

#[test]
fn move_patches_wef_column_and_state() {
    let board = stories_board();
    let card = cards()[&111354].clone();
    let plan = MovePlan::new(&board, &card, &target(&board, "Blocked")).unwrap();
    let patch = serde_json::to_value(plan.to_patch().unwrap()).unwrap();
    assert_eq!(
        patch,
        json!([
            { "op": "test", "path": "/rev", "value": 11 },
            { "op": "add", "path": "/fields/WEF_05102A5E6E294938B163F2FBD09ECDF2_Kanban.Column", "value": "Blocked" },
            { "op": "add", "path": "/fields/System.State", "value": "Blocked" }
        ])
    );
    assert!(!patch.to_string().contains("System.BoardColumn"));
}

#[test]
fn reorder_within_column_has_no_patch() {
    let board = stories_board();
    let card = cards()[&111354].clone();
    let mut t = target(&board, "Paused");
    t.next_id = Some(1);
    let plan = MovePlan::new(&board, &card, &t).unwrap();
    assert!(plan.to_patch().is_none());
    assert_eq!(plan.reorder, Some((None, Some(1))));
}

#[test]
fn state_not_patched_when_mapping_matches() {
    let board = stories_board();
    let mut card = cards()[&111354].clone();
    card.board_column = Some("Active".into()); // column and state out of sync
    let plan = MovePlan::new(&board, &card, &target(&board, "Paused")).unwrap();
    assert_eq!(plan.field_changes.len(), 1);
    assert!(plan.new_state.is_none());
}

#[test]
fn wrong_type_is_rejected() {
    let board = stories_board();
    let epic = cards()[&118254].clone();
    assert!(matches!(
        MovePlan::new(&board, &epic, &target(&board, "Active")),
        Err(MoveError::TypeNotAllowed { .. })
    ));
}

#[test]
fn wip_limit_warns() {
    let mut board = stories_board();
    let active = board
        .columns
        .iter()
        .find(|c| c.name == "Active")
        .unwrap()
        .id
        .clone();
    board.placement.insert(active, vec![1, 2, 3, 4, 5]);
    let card = cards()[&111354].clone();
    let plan = MovePlan::new(&board, &card, &target(&board, "Active")).unwrap();
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn optimistic_apply_and_reposition() {
    let mut board = stories_board();
    let mut card = cards()[&111354].clone();
    let t = target(&board, "Blocked");
    MovePlan::new(&board, &card, &t).unwrap().apply(&mut card);
    assert_eq!(card.state, "Blocked");
    assert_eq!(card.board_column.as_deref(), Some("Blocked"));
    board.reposition(card.id, &t.column_id, 0);
    assert_eq!(board.placement[&t.column_id], vec![111354]);
    assert_eq!(board.neighbours(&t.column_id, 999, 1), (Some(111354), None));
}

#[test]
fn backlog_levels_and_state_categories() {
    let cfg: BacklogConfigurationDto = fixture("backlog_configuration.json");
    let levels = levels_of(&cfg);
    assert_eq!(
        levels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        ["Epics", "Features", "Stories"]
    );
    let states = StateMap::from_backlog(&cfg);
    assert_eq!(
        states.category("User Story", "TEST-ACTIVE"),
        StateCategory::Resolved
    );
    assert_eq!(
        states.category("Feature", "Resolved"),
        StateCategory::InProgress
    );
    assert_eq!(
        states.states("User Story").first().map(String::as_str),
        Some("New")
    );
}

#[test]
fn hierarchy_and_progress() {
    let cfg: BacklogConfigurationDto = fixture("backlog_configuration.json");
    let states = StateMap::from_backlog(&cfg);
    let cards = cards();
    let h = Hierarchy::build(&cards, &["Epic".into()]);
    assert_eq!(h.roots, vec![118254]);
    assert_eq!(h.children[&118254], vec![(118258, 1)]);
    assert_eq!(h.root_of.get(&118258), Some(&118254));
    assert_eq!(h.progress(118254, &cards, &states), (1, 1)); // Done is Resolved
}

#[test]
fn card_settings_fields_are_fetched() {
    let cs: CardSettingsDto = fixture("card_settings_stories.json");
    let f = fields_to_fetch(ORDER, &cs.field_identifiers());
    assert!(f.contains(&"Microsoft.VSTS.Scheduling.StoryPoints".to_owned()));
    assert!(f.contains(&ORDER.to_owned()));
}

#[test]
fn state_colors_from_ado_with_category_fallback() {
    let cfg: BacklogConfigurationDto = fixture("backlog_configuration.json");
    let mut states = StateMap::from_backlog(&cfg);
    assert_eq!(states.color("User Story", "Blocked"), "007acc"); // no colors yet: InProgress
    let dtos: ListResponse<ado_client::WorkItemStateDto> =
        fixture("workitemtype_states_story.json");
    states.set_colors("User Story", &dtos.value);
    assert_eq!(states.color("User Story", "Blocked"), "b20b1e");
    assert_eq!(states.color("User Story", "Done"), "ff9d00");
    assert_eq!(states.color("Feature", "Active"), "007acc");
}

#[test]
fn type_colors_from_ado() {
    let dtos: ListResponse<ado_client::WorkItemTypeDto> = fixture("workitemtypes.json");
    let colors = kanban_core::workspace::type_colors(&dtos.value);
    assert_eq!(colors["Epic"], "e06c00");
    assert_eq!(colors["User Story"], "0098c7");
}
