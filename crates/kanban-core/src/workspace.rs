//! Everything one project/team screen needs, loaded in one go: backlog levels, state categories
//! and colors, type colors, each level's board, the cards of every level and their tasks,
//! iterations and the signed-in user.
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
    /// Types of the task backlog (default first), plus Bug when bugs behave as tasks. Not a
    /// level: tasks have no board and are shown under their parent.
    #[serde(default)]
    pub task_types: Vec<String>,
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
    /// Work item type → color (hex without `#`), as ADO shows it.
    #[serde(default)]
    pub type_colors: HashMap<String, String>,
    /// Work item type → ADO icon id (`icon_book`, …), for the optional List type icons.
    #[serde(default)]
    pub type_icons: HashMap<String, String>,
    /// Type name → its Azure DevOps icon in the type colour, as a `data:image/svg+xml` URI
    /// (fetched at load; kept in the cache). Missing ones fall back to the app's own drawings.
    #[serde(default)]
    pub type_icon_svgs: HashMap<String, String>,
    /// Newest `System.ChangedDate` seen; the next delta sync starts here.
    pub synced_to: Option<OffsetDateTime>,
}

impl Workspace {
    pub async fn load(client: &AdoClient, project: &str, team: &str) -> Result<Self, CoreError> {
        let (backlog, team_values, iterations, me, types) = tokio::join!(
            client.backlog_configuration(project, team),
            client.team_field_values(project, team),
            client.iterations(project, team, false),
            client.connection_data(),
            client.work_item_types(project),
        );
        let backlog = backlog.map_err(|e| match e {
            ado_client::AdoError::Api {
                status, message, ..
            } if status >= 500 => CoreError::BoardUnavailable(format!("{team}: {message}")),
            other => CoreError::Ado(other),
        })?;
        let team_values = team_values?;
        let levels = levels_of(&backlog);
        let task_types = task_types_of(&backlog);
        let mut states = StateMap::from_backlog(&backlog);

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
            .chain(task_types.iter().cloned())
            .collect();
        work_item_types.sort();
        work_item_types.dedup();

        // State colors per type, concurrently. Without them pills fall back to category colors.
        let mut set = JoinSet::new();
        for t in &work_item_types {
            let (client, project, t) = (client.clone(), project.to_owned(), t.clone());
            set.spawn(async move {
                let res = client.work_item_type_states(&project, &t).await;
                (t, res)
            });
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((t, Ok(dtos))) => states.set_colors(&t, &dtos),
                Ok((t, Err(e))) => {
                    tracing::warn!(work_item_type = %t, error = %e, "no state colors")
                }
                Err(_) => {}
            }
        }
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
            task_types,
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
                    descriptor: u.subject_descriptor.clone(),
                })
            }),
            type_colors: types
                .as_ref()
                .inspect_err(|e| tracing::warn!(error = %e, "no work item type colors"))
                .map(|t| type_colors(t))
                .unwrap_or_default(),
            type_icons: types.as_ref().map(|t| type_icons(t)).unwrap_or_default(),
            type_icon_svgs: HashMap::new(),
            synced_to: None,
        };
        ws.type_icon_svgs = fetch_type_icons(client, &ws.type_icons, &ws.type_colors).await;
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
        Hierarchy::build(&self.cards, &self.root_types(), &self.task_types)
    }

    pub fn is_task(&self, card: &Card) -> bool {
        self.task_types.contains(&card.work_item_type)
    }

    /// The type new tasks are created as.
    pub fn task_type(&self) -> Option<&str> {
        self.task_types.first().map(String::as_str)
    }

    /// Can the card have tasks? Requirement-level items (stories, and bugs on that level).
    pub fn owns_tasks(&self, card: &Card) -> bool {
        !self.task_types.is_empty()
            && self
                .levels
                .iter()
                .find(|l| !l.is_portfolio)
                .is_some_and(|l| l.work_item_types.contains(&card.work_item_type))
    }

    pub fn category(&self, card: &Card) -> StateCategory {
        self.states.category(&card.work_item_type, &card.state)
    }

    /// The type's ADO icon id, when ADO reported one.
    pub fn type_icon(&self, work_item_type: &str) -> Option<String> {
        self.type_icons.get(work_item_type).cloned()
    }

    /// The type's icon as Azure DevOps draws it (a `data:` URI), when it could be fetched.
    pub fn type_icon_src(&self, work_item_type: &str) -> Option<String> {
        self.type_icon_svgs.get(work_item_type).cloned()
    }

    /// The type's ADO color (hex without `#`), gray when unknown.
    pub fn type_color(&self, work_item_type: &str) -> String {
        self.type_colors
            .get(work_item_type)
            .cloned()
            .unwrap_or_else(|| StateCategory::Unknown.default_color().to_owned())
    }

    /// The card's state color (hex without `#`).
    pub fn state_color(&self, card: &Card) -> String {
        self.states.color(&card.work_item_type, &card.state)
    }

    /// The team's current sprint.
    pub fn current_iteration(&self) -> Option<&Iteration> {
        self.iterations.iter().find(|i| i.current)
    }

    /// Types an item of `work_item_type` can hang under (see [`parent_types`]).
    pub fn parent_types(&self, work_item_type: &str) -> Vec<String> {
        parent_types(&self.levels, &self.task_types, work_item_type)
    }

    /// The board column a `work_item_type` item in `state` shows in, by the board's state
    /// mappings.
    pub fn column_for_state(&self, work_item_type: &str, state: &str) -> Option<String> {
        let level = self.level_of_type(work_item_type).map(|l| l.name.as_str());
        let board = match level {
            Some(l) => self.boards.get(l),
            // Tasks have no board of their own.
            None => None,
        }?;
        board
            .column_for_state(work_item_type, state)
            .map(|c| c.name.clone())
    }

    /// Tags on the loaded cards with how many cards use each, most used first. Tags compare
    /// case-insensitively, as in ADO; the first spelling seen wins.
    pub fn tag_counts(&self) -> Vec<(String, usize)> {
        let mut by_key: HashMap<String, (String, usize)> = HashMap::new();
        for tag in self.cards.values().flat_map(|c| &c.tags) {
            by_key
                .entry(tag.to_lowercase())
                .or_insert_with(|| (tag.clone(), 0))
                .1 += 1;
        }
        let mut out: Vec<(String, usize)> = by_key.into_values().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// The level a work item type belongs to.
    pub fn level_of_type(&self, work_item_type: &str) -> Option<&BacklogLevel> {
        self.levels
            .iter()
            .find(|l| l.work_item_types.iter().any(|t| t == work_item_type))
    }
}

/// Types an item of `work_item_type` can hang under: for tasks (and bugs shown as tasks) the
/// requirement level, for backlog items every level above their own (a story under a feature
/// or an epic). Empty for the top level.
pub fn parent_types(
    levels: &[BacklogLevel],
    task_types: &[String],
    work_item_type: &str,
) -> Vec<String> {
    if task_types.iter().any(|t| t == work_item_type) {
        return levels
            .iter()
            .find(|l| !l.is_portfolio)
            .map(|l| l.work_item_types.clone())
            .unwrap_or_default();
    }
    let Some(i) = levels
        .iter()
        .position(|l| l.work_item_types.iter().any(|t| t == work_item_type))
    else {
        return Vec::new();
    };
    levels[..i]
        .iter()
        .flat_map(|l| l.work_item_types.iter().cloned())
        .collect()
}

/// Fetches each type's icon in its colour, concurrently. Types whose icon fails keep none (the
/// app draws its own); a few small requests per load.
async fn fetch_type_icons(
    client: &AdoClient,
    icons: &HashMap<String, String>,
    colors: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut set = JoinSet::new();
    for (ty, icon) in icons {
        let (client, ty, icon) = (client.clone(), ty.clone(), icon.clone());
        let color = colors.get(&ty).cloned().unwrap_or_else(|| "6b7686".into());
        set.spawn(async move { (ty, client.work_item_icon_svg(&icon, &color).await) });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((ty, Ok(svg))) => match svg_data_uri(&svg) {
                Some(uri) => {
                    out.insert(ty, uri);
                }
                None => tracing::warn!(work_item_type = %ty, "type icon is not SVG"),
            },
            Ok((ty, Err(e))) => tracing::warn!(work_item_type = %ty, error = %e, "no type icon"),
            Err(_) => {}
        }
    }
    out
}

/// `svg` as a `data:image/svg+xml;base64,…` URI, or `None` if it isn't SVG. Shown through an
/// `<img>`, which never runs scripts inside the SVG.
pub fn svg_data_uri(svg: &str) -> Option<String> {
    use base64::Engine;
    let t = svg.trim_start();
    (t.starts_with("<svg") || (t.starts_with("<?xml") && t.contains("<svg"))).then(|| {
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(t)
        )
    })
}

/// Type name → color (hex without `#`, lowercase).
pub fn type_colors(types: &[ado_client::WorkItemTypeDto]) -> HashMap<String, String> {
    types
        .iter()
        .filter_map(|t| Some((t.name.clone(), t.color.clone()?.to_ascii_lowercase())))
        .collect()
}

/// Type name → ADO icon id.
pub fn type_icons(types: &[ado_client::WorkItemTypeDto]) -> HashMap<String, String> {
    types
        .iter()
        .filter_map(|t| Some((t.name.clone(), t.icon.as_ref()?.id.clone())))
        .collect()
}

/// The task backlog's types (default first), plus Bug when `bugsBehavior` is `asTasks`.
pub fn task_types_of(cfg: &BacklogConfigurationDto) -> Vec<String> {
    let level = &cfg.task_backlog;
    let mut out: Vec<String> = level
        .default_work_item_type
        .iter()
        .map(|t| t.name.clone())
        .chain(level.work_item_types.iter().map(|t| t.name.clone()))
        .collect();
    if cfg.bugs_behavior.as_deref() == Some("asTasks") {
        out.push("Bug".into());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|t| seen.insert(t.clone()));
    out
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
