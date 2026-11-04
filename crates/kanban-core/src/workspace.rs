//! Everything one project/team screen needs, loaded in one go: backlog levels, state categories,
//! each level's board, the cards of every level, iterations and the signed-in user.
//!
//! A board that ADO can't serve (some teams always get HTTP 500) is recorded in `board_errors`;
//! the rest of the workspace still loads.

use std::collections::HashMap;

use ado_client::{AdoClient, BacklogConfigurationDto, WorkItemDto};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::task::JoinSet;

use crate::board::Board;
use crate::card::{Card, CardId, Person, fields_to_fetch};
use crate::error::CoreError;
use crate::hierarchy::Hierarchy;
use crate::queries::Scope;
use crate::states::{StateCategory, StateMap};

/// One backlog level (Epics, Features, Stories).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacklogLevel {
    pub name: String,
    pub rank: u32,
    pub work_item_types: Vec<String>,
    pub default_type: Option<String>,
    pub is_portfolio: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Iteration {
    pub name: String,
    pub path: String,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub project: String,
    pub team: String,
    /// Visible levels, highest rank (Epics) first.
    pub levels: Vec<BacklogLevel>,
    pub states: StateMap,
    pub scope: Scope,
    pub fields: Vec<String>,
    pub cards: HashMap<CardId, Card>,
    /// Board per level name.
    pub boards: HashMap<String, Board>,
    pub board_errors: HashMap<String, String>,
    pub iterations: Vec<Iteration>,
    /// Team field values (area paths) for pickers.
    pub areas: Vec<String>,
    pub me: Option<Person>,
    /// Newest `System.ChangedDate` seen; the next delta sync starts here.
    pub synced_to: Option<OffsetDateTime>,
}

impl Workspace {
    pub async fn load(client: &AdoClient, project: &str, team: &str) -> Result<Self, CoreError> {
        let (backlog, team_values, iterations, me) = tokio::join!(
            client.backlog_configuration(project, team),
            client.team_field_values(project, team),
            client.iterations(project, team, false),
            client.connection_data(),
        );
        let backlog = backlog.map_err(|e| match e {
            ado_client::AdoError::Api {
                status, message, ..
            } if status >= 500 => CoreError::BoardUnavailable(format!("{team}: {message}")),
            other => CoreError::Ado(other),
        })?;
        let team_values = team_values?;
        let levels = levels_of(&backlog);
        let states = StateMap::from_backlog(&backlog);

        // Board config per level, concurrently. Failures are per board.
        let mut set = JoinSet::new();
        for level in &levels {
            let (client, project, team, name) = (
                client.clone(),
                project.to_owned(),
                team.to_owned(),
                level.name.clone(),
            );
            set.spawn(async move {
                let board = client.board(&project, &team, &name).await;
                let cards = client.card_settings(&project, &team, &name).await;
                (name, board, cards)
            });
        }
        let mut boards = HashMap::new();
        let mut board_errors = HashMap::new();
        let mut card_fields = Vec::new();
        while let Some(joined) = set.join_next().await {
            let Ok((name, board, card_settings)) = joined else {
                continue;
            };
            match board {
                Ok(dto) => {
                    boards.insert(name, Board::from_dto(project, team, &dto));
                }
                Err(e) => {
                    tracing::warn!(board = %name, error = %e, "board unavailable");
                    board_errors.insert(name, e.to_string());
                }
            }
            if let Ok(cs) = card_settings {
                card_fields.extend(cs.field_identifiers());
            }
        }

        let mut work_item_types: Vec<String> = levels
            .iter()
            .flat_map(|l| l.work_item_types.iter().cloned())
            .chain(
                boards
                    .values()
                    .flat_map(|b| b.work_item_types.iter().cloned()),
            )
            .collect();
        work_item_types.sort();
        work_item_types.dedup();
        let order_field = backlog
            .backlog_fields
            .type_fields
            .get("Order")
            .cloned()
            .unwrap_or_else(|| "Microsoft.VSTS.Common.StackRank".into());
        let scope = Scope {
            project: project.to_owned(),
            team: team.to_owned(),
            team_field: team_values.field.reference_name.clone(),
            team_values: Scope::team_values_from(&team_values),
            completed_states: states.states_in(&work_item_types, StateCategory::Completed),
            work_item_types,
            order_field: order_field.clone(),
        };
        let fields = fields_to_fetch(&order_field, &card_fields);

        let ids = client.wiql(project, team, &scope.items_query()).await?;
        let dtos = client.work_items_batch(project, &ids, &fields).await?;

        let mut ws = Self {
            project: project.to_owned(),
            team: team.to_owned(),
            levels,
            states,
            areas: team_values.values.iter().map(|v| v.value.clone()).collect(),
            scope,
            fields,
            cards: HashMap::new(),
            boards,
            board_errors,
            iterations: iterations
                .unwrap_or_default()
                .into_iter()
                .map(|i| Iteration {
                    current: i.attributes.time_frame.as_deref() == Some("current"),
                    name: i.name,
                    path: i.path,
                })
                .collect(),
            me: me.ok().and_then(|c| {
                let u = c.authenticated_user;
                Some(Person {
                    display_name: u.display_name(),
                    unique_name: u.account()?,
                })
            }),
            synced_to: None,
        };
        ws.upsert(&dtos);
        ws.synced_to = ws.cards.values().filter_map(|c| c.changed_date).max();
        Ok(ws)
    }

    /// Applies fetched items: inserts/replaces those in scope, drops those that left it.
    /// Returns the ids that changed.
    pub fn upsert(&mut self, dtos: &[WorkItemDto]) -> Vec<CardId> {
        let mut changed = Vec::new();
        for dto in dtos {
            let card = Card::from_work_item(dto, &self.scope.order_field);
            if let Some(d) = card.changed_date {
                self.synced_to = Some(self.synced_to.map_or(d, |s| s.max(d)));
            }
            let id = card.id;
            if self.scope.contains(&card) {
                if self.cards.get(&id) != Some(&card) {
                    self.cards.insert(id, card);
                    changed.push(id);
                }
            } else if self.cards.remove(&id).is_some() {
                changed.push(id);
            }
        }
        if !changed.is_empty() {
            self.place();
        }
        changed
    }

    /// Recomputes every board's placement from `cards`.
    pub fn place(&mut self) {
        for board in self.boards.values_mut() {
            board.place(self.cards.values());
        }
    }

    /// Types at the top of the epic list (the highest visible portfolio level).
    pub fn root_types(&self) -> Vec<String> {
        self.levels
            .iter()
            .find(|l| l.is_portfolio)
            .map(|l| l.work_item_types.clone())
            .unwrap_or_default()
    }

    /// Levels the kanban can switch between: every level below the root, that has a board.
    pub fn kanban_levels(&self) -> Vec<&BacklogLevel> {
        let root = self
            .levels
            .iter()
            .find(|l| l.is_portfolio)
            .map(|l| l.name.as_str());
        self.levels
            .iter()
            .filter(|l| Some(l.name.as_str()) != root)
            .filter(|l| {
                self.boards.contains_key(&l.name) || self.board_errors.contains_key(&l.name)
            })
            .collect()
    }

    pub fn hierarchy(&self) -> Hierarchy {
        Hierarchy::build(&self.cards, &self.root_types())
    }

    pub fn category(&self, card: &Card) -> StateCategory {
        self.states.category(&card.work_item_type, &card.state)
    }

    /// The level a work item type belongs to.
    pub fn level_of_type(&self, work_item_type: &str) -> Option<&BacklogLevel> {
        self.levels
            .iter()
            .find(|l| l.work_item_types.iter().any(|t| t == work_item_type))
    }
}

/// Visible portfolio levels (highest rank first) followed by the requirement level.
pub fn levels_of(cfg: &BacklogConfigurationDto) -> Vec<BacklogLevel> {
    let mut portfolio: Vec<BacklogLevel> = cfg
        .portfolio_backlogs
        .iter()
        .filter(|l| !l.is_hidden.unwrap_or(false))
        .map(|l| level(l, true))
        .collect();
    portfolio.sort_by_key(|l| std::cmp::Reverse(l.rank));
    portfolio.push(level(&cfg.requirement_backlog, false));
    // asRequirements puts Bugs on the requirement board.
    if cfg.bugs_behavior.as_deref() == Some("asRequirements")
        && let Some(req) = portfolio.last_mut()
        && !req.work_item_types.iter().any(|t| t == "Bug")
    {
        req.work_item_types.push("Bug".into());
    }
    portfolio
}

fn level(dto: &ado_client::BacklogLevelDto, is_portfolio: bool) -> BacklogLevel {
    BacklogLevel {
        name: dto.name.clone(),
        rank: dto.rank,
        work_item_types: dto.work_item_types.iter().map(|t| t.name.clone()).collect(),
        default_type: dto.default_work_item_type.as_ref().map(|t| t.name.clone()),
        is_portfolio,
    }
}
