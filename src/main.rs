use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod gui;

#[cfg(test)]
const DEFAULT_BOARD_SIZE: usize = 11;
const MIN_BOARD_SIZE: usize = 11;
const MAX_BOARD_SIZE: usize = 101;
const DEFAULT_MAP: &str = include_str!("../maps/crossroads.map");
const PLAYER_COUNT: usize = 4;
const DEFAULT_TICKS: u32 = 200;
const DEFAULT_FRAME_DELAY_MS: u64 = 120;
const MAX_AP: u8 = 6;
const RECENT_ACTIVITY_LIMIT: usize = 20;
const UNSUPPLIED_TICK_LIMIT: u8 = 5;
const AP_PURCHASE_COSTS: [u32; 5] = [20, 40, 80, 160, 320];
const STANCE_COUNT: usize = 3;
const STANCE_NAMES: [&str; STANCE_COUNT] = ["Explorer", "Aggro", "Base-builder"];

fn is_valid_board_size(board_size: usize) -> bool {
    (MIN_BOARD_SIZE..=MAX_BOARD_SIZE).contains(&board_size) && board_size % 2 == 1
}

struct Options {
    batch: Option<usize>,
    ticks: u32,
    frame_delay_ms: u64,
    generated_size: Option<usize>,
    map_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stance {
    Explorer,
    Aggro,
    BaseBuilder,
}

impl Stance {
    fn from_index(index: usize) -> Self {
        match index % STANCE_COUNT {
            0 => Self::Explorer,
            1 => Self::Aggro,
            _ => Self::BaseBuilder,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Explorer => 0,
            Self::Aggro => 1,
            Self::BaseBuilder => 2,
        }
    }

    fn name(self) -> &'static str {
        STANCE_NAMES[self.index()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    owner: Option<usize>,
    base: bool,
    wall: bool,
    outpost: bool,
    fortification: u8,
    unsupplied_ticks: u8,
}

impl Cell {
    fn neutral() -> Self {
        Self {
            owner: None,
            base: false,
            wall: false,
            outpost: false,
            fortification: 0,
            unsupplied_ticks: 0,
        }
    }
}

#[derive(Clone)]
struct MapDefinition {
    name: String,
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    bases: [usize; PLAYER_COUNT],
}

impl MapDefinition {
    fn parse(source: &str) -> Result<Self, String> {
        let rows: Vec<Vec<char>> = source
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
                    None
                } else {
                    Some(line.chars().filter(|ch| !ch.is_whitespace()).collect())
                }
            })
            .collect();
        let height = rows.len();
        let width = rows.first().map_or(0, Vec::len);
        if !is_valid_board_size(height) || !is_valid_board_size(width) {
            return Err(format!(
                "map width and height must be odd numbers between {MIN_BOARD_SIZE} and {MAX_BOARD_SIZE}"
            ));
        }

        let mut cells = vec![Cell::neutral(); width * height];
        let mut bases = [None; PLAYER_COUNT];
        for (y, row) in rows.iter().enumerate() {
            if row.len() != width {
                return Err(format!(
                    "map row {} has {} cells; expected {width}",
                    y + 1,
                    row.len()
                ));
            }
            for (x, tile) in row.iter().enumerate() {
                let index = Game::index(x, y, width);
                match tile {
                    '.' => {}
                    '#' => cells[index].wall = true,
                    'A'..='D' => {
                        let player = (*tile as u8 - b'A') as usize;
                        if bases[player].replace(index).is_some() {
                            return Err(format!("map contains base {tile} more than once"));
                        }
                        cells[index].owner = Some(player);
                        cells[index].base = true;
                    }
                    _ => {
                        return Err(format!(
                            "map contains unsupported character {tile:?} at row {}, column {}",
                            y + 1,
                            x + 1
                        ));
                    }
                }
            }
        }
        let bases = bases.map(|base| {
            base.ok_or_else(|| {
                "map must contain exactly one base for each player: A, B, C and D".to_string()
            })
        });
        let bases = bases.into_iter().collect::<Result<Vec<_>, _>>()?;
        let bases: [usize; PLAYER_COUNT] =
            bases.try_into().expect("exactly four bases were collected");

        Ok(Self {
            name: "text map".to_string(),
            width,
            height,
            cells,
            bases,
        })
    }

    fn generated(size: usize) -> Self {
        assert!(is_valid_board_size(size));
        let mut cells = vec![Cell::neutral(); size * size];
        let center = size / 2;
        for y in [center - 3, center - 2, center, center + 2, center + 3] {
            cells[Game::index(center, y, size)].wall = true;
        }
        for x in [center - 3, center - 2, center, center + 2, center + 3] {
            cells[Game::index(x, center, size)].wall = true;
        }
        let base_positions = [(1, 1), (size - 2, 1), (1, size - 2), (size - 2, size - 2)];
        let bases = base_positions.map(|(x, y)| Game::index(x, y, size));
        Self {
            name: format!("generated {size}x{size}"),
            width: size,
            height: size,
            cells,
            bases,
        }
    }
}

#[derive(Clone, Debug)]
struct Player {
    name: char,
    stance: Stance,
    base: usize,
    ap: u8,
    fields_won_in_combat: u32,
    fields_taken_by_discovery: u32,
    attacks: u32,
    outposts_built: u32,
    outposts_lost: u32,
    produced: u64,
    spent: u64,
}

#[derive(Clone, Debug)]
struct Network {
    owner: usize,
    stock: u32,
    cells: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
enum ActionKind {
    Claim,
    Attack { strength: u8 },
    BuildOutpost,
    Fortify,
}

#[derive(Clone, Copy, Debug)]
struct Action {
    origin: usize,
    target: usize,
    kind: ActionKind,
    cost: u32,
}

#[derive(Default)]
struct ChoiceCounts {
    neutral: usize,
    enemy: usize,
    outpost: usize,
    fortify: usize,
}

#[derive(Clone)]
struct Game {
    map_name: String,
    board_width: usize,
    board_height: usize,
    cells: Vec<Cell>,
    players: Vec<Player>,
    networks: BTreeMap<usize, Network>,
    cell_network: Vec<Option<usize>>,
    base_distances: Vec<Vec<usize>>,
    last_tick_targets: Vec<HashSet<usize>>,
    last_action_sources: HashSet<usize>,
    last_action_targets: HashSet<usize>,
    outpost_strain: HashMap<usize, u8>,
    log: VecDeque<String>,
    activity_log: Vec<String>,
    choice_log: Vec<String>,
    tick: u32,
    max_ticks: u32,
    simulation_no: usize,
}

impl Game {
    #[cfg(test)]
    fn new(simulation_no: usize, max_ticks: u32) -> Self {
        Self::from_map(
            simulation_no,
            max_ticks,
            MapDefinition::generated(DEFAULT_BOARD_SIZE),
        )
    }

    #[cfg(test)]
    fn with_board_size(simulation_no: usize, max_ticks: u32, board_size: usize) -> Self {
        Self::from_map(
            simulation_no,
            max_ticks,
            MapDefinition::generated(board_size),
        )
    }

    fn from_map(simulation_no: usize, max_ticks: u32, map: MapDefinition) -> Self {
        let map_name = map.name.clone();
        let board_width = map.width;
        let board_height = map.height;
        let mut cells = map.cells;
        let mut players = Vec::with_capacity(PLAYER_COUNT);
        for (index, base) in map.bases.into_iter().enumerate() {
            cells[base].owner = Some(index);
            cells[base].base = true;
            players.push(Player {
                name: (b'A' + index as u8) as char,
                stance: Self::stance_for_seat(simulation_no, index),
                base,
                ap: MAX_AP,
                fields_won_in_combat: 0,
                fields_taken_by_discovery: 0,
                attacks: 0,
                outposts_built: 0,
                outposts_lost: 0,
                produced: 0,
                spent: 0,
            });
        }

        let mut game = Self {
            map_name,
            board_width,
            board_height,
            cells,
            players,
            networks: BTreeMap::new(),
            cell_network: vec![None; board_width * board_height],
            base_distances: Vec::new(),
            last_tick_targets: vec![HashSet::new(); PLAYER_COUNT],
            last_action_sources: HashSet::new(),
            last_action_targets: HashSet::new(),
            outpost_strain: HashMap::new(),
            log: VecDeque::new(),
            activity_log: Vec::new(),
            choice_log: Vec::new(),
            tick: 0,
            max_ticks: max_ticks.max(1),
            simulation_no,
        };
        game.base_distances = game
            .players
            .iter()
            .map(|player| game.distances_from(player.base))
            .collect();
        game.refresh_networks();
        for network in game.networks.values_mut() {
            network.stock = 12;
        }
        game.log(format!(
            "Simulation {} on {} ({}x{}), {} ticks, duplicate stance {}; bases: {}",
            simulation_no + 1,
            game.map_name,
            game.board_width,
            game.board_height,
            game.max_ticks,
            Stance::from_index(simulation_no / 4).name(),
            game.players
                .iter()
                .map(|player| { format!("{} {}", player.name, game.label(player.base)) })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        game
    }

    fn stance_for_seat(simulation_no: usize, seat: usize) -> Stance {
        let duplicate = (simulation_no / 4) % STANCE_COUNT;
        let rotation = simulation_no % 4;
        let mut stances = [
            Stance::from_index(duplicate),
            Stance::from_index(duplicate),
            Stance::from_index(duplicate + 1),
            Stance::from_index(duplicate + 2),
        ];
        stances.rotate_right(rotation);
        stances[seat]
    }

    fn index(x: usize, y: usize, board_size: usize) -> usize {
        y * board_size + x
    }

    fn xy(index: usize, board_size: usize) -> (usize, usize) {
        (index % board_size, index / board_size)
    }

    fn neighbors(&self, cell: usize) -> Vec<usize> {
        self.adjacent_cells(cell)
            .into_iter()
            .filter(|&neighbor| !self.cells[neighbor].wall)
            .collect()
    }

    fn adjacent_cells(&self, cell: usize) -> Vec<usize> {
        let (x, y) = Self::xy(cell, self.board_width);
        let mut neighbors = Vec::with_capacity(4);
        if x > 0 {
            neighbors.push(Self::index(x - 1, y, self.board_width));
        }
        if x + 1 < self.board_width {
            neighbors.push(Self::index(x + 1, y, self.board_width));
        }
        if y > 0 {
            neighbors.push(Self::index(x, y - 1, self.board_width));
        }
        if y + 1 < self.board_height {
            neighbors.push(Self::index(x, y + 1, self.board_width));
        }
        neighbors
    }

    fn log(&mut self, message: impl Into<String>) {
        let entry = format!("T{:03} {}", self.tick, message.into());
        self.activity_log.push(entry.clone());
        self.log.push_front(entry);
        self.log.truncate(80);
    }

    fn write_logs(&self, directory: &Path) -> io::Result<()> {
        let game_id = self.simulation_no + 1;
        fs::write(
            directory.join(format!("game-{game_id:04}-activity.log")),
            format!("{}\n", self.activity_log.join("\n")),
        )?;
        fs::write(
            directory.join(format!("game-{game_id:04}-choices.log")),
            format!("{}\n", self.choice_log.join("\n")),
        )
    }

    fn score(&self, player: usize) -> usize {
        self.cells
            .iter()
            .zip(&self.cell_network)
            .filter(|(cell, network)| cell.owner == Some(player) && network.is_some())
            .count()
    }

    fn total_fields(&self, player: usize) -> usize {
        self.cells
            .iter()
            .filter(|cell| cell.owner == Some(player))
            .count()
    }

    fn describe_action(&self, action: Action) -> String {
        let kind = match action.kind {
            ActionKind::Claim => "claim".to_string(),
            ActionKind::Attack { strength } => format!("attack/{strength}"),
            ActionKind::BuildOutpost => "outpost".to_string(),
            ActionKind::Fortify => "fortify".to_string(),
        };
        format!("{kind}@{}", self.label(action.target))
    }

    fn available_choice_counts(
        &self,
        player: usize,
        unavailable_targets: &HashSet<usize>,
    ) -> ChoiceCounts {
        let mut neutral = BTreeSet::new();
        let mut enemy = BTreeSet::new();
        let mut outpost = BTreeSet::new();
        let mut fortify = BTreeSet::new();
        for (origin, network_id) in self.cell_network.iter().enumerate() {
            if self.cells[origin].owner != Some(player) || network_id.is_none() {
                continue;
            }
            let stock = self.networks[&network_id.unwrap()].stock;
            for target in self.neighbors(origin) {
                if unavailable_targets.contains(&target) {
                    continue;
                }
                match self.cells[target].owner {
                    None if stock >= 2 => {
                        neutral.insert(target);
                    }
                    Some(owner) if owner != player && !self.cells[target].base => {
                        if Self::attack_cost(
                            self.defense_strength(target, owner),
                            stock,
                            self.players[player].stance,
                        )
                        .is_some()
                        {
                            enemy.insert(target);
                        }
                    }
                    _ => {}
                }
            }
        }

        if self.players[player].stance == Stance::BaseBuilder
            && self
                .cells
                .iter()
                .filter(|cell| cell.owner == Some(player) && cell.outpost)
                .count()
                < 2
        {
            for (cell, network_id) in self.cell_network.iter().enumerate() {
                if unavailable_targets.contains(&cell)
                    || self.cells[cell].owner != Some(player)
                    || self.cells[cell].base
                    || self.cells[cell].outpost
                    || self
                        .neighbors(cell)
                        .iter()
                        .filter(|&&neighbor| self.cells[neighbor].owner == Some(player))
                        .count()
                        < 3
                {
                    continue;
                }
                if network_id.is_some_and(|network_id| self.networks[&network_id].stock >= 15) {
                    outpost.insert(cell);
                }
            }
        }

        if self.players[player].stance == Stance::BaseBuilder {
            for (cell, network_id) in self.cell_network.iter().enumerate() {
                if unavailable_targets.contains(&cell)
                    || self.cells[cell].owner != Some(player)
                    || self.cells[cell].fortification != 0
                    || !self.neighbors(cell).iter().any(|&neighbor| {
                        self.cells[neighbor]
                            .owner
                            .is_some_and(|owner| owner != player)
                    })
                {
                    continue;
                }
                if network_id.is_some_and(|network_id| self.networks[&network_id].stock >= 3) {
                    fortify.insert(cell);
                }
            }
        }

        ChoiceCounts {
            neutral: neutral.len(),
            enemy: enemy.len(),
            outpost: outpost.len(),
            fortify: fortify.len(),
        }
    }

    fn is_finished(&self) -> bool {
        self.tick >= self.max_ticks
    }

    fn tick(&mut self) {
        self.advance_tick(None);
    }

    #[cfg(test)]
    fn tick_frames(&mut self) -> Vec<Self> {
        let mut frames = Vec::new();
        self.advance_tick(Some(&mut frames));
        frames
    }

    fn advance_tick(&mut self, frames: Option<&mut Vec<Self>>) {
        if self.is_finished() {
            return;
        }
        let fields_before: Vec<_> = (0..PLAYER_COUNT)
            .map(|player| self.total_fields(player))
            .collect();
        self.tick += 1;
        self.last_action_sources.clear();
        self.last_action_targets.clear();
        for player in &mut self.players {
            player.ap = (player.ap + 1).min(MAX_AP);
        }
        self.produce();

        let ap_at_submission: Vec<_> = self.players.iter().map(|player| player.ap).collect();
        let choice_counts: Vec<_> = (0..PLAYER_COUNT)
            .map(|player| self.available_choice_counts(player, &self.last_tick_targets[player]))
            .collect();
        let mut orders = Vec::new();
        for player in 0..PLAYER_COUNT {
            orders.extend(
                self.plan_player_orders(player)
                    .into_iter()
                    .map(|action| (player, action)),
            );
        }
        self.resolve_orders(orders.clone());
        self.capture_surrounded_neutral_areas();

        self.check_outposts();
        self.expire_unsupplied_fields();
        for player in 0..PLAYER_COUNT {
            let chosen = orders
                .iter()
                .filter(|(order_player, _)| *order_player == player)
                .map(|(_, action)| self.describe_action(*action))
                .collect::<Vec<_>>();
            let counts = &choice_counts[player];
            let net = self.total_fields(player) as i32 - fields_before[player] as i32;
            self.choice_log.push(format!(
                "T{:04} {}: AP={} neutral={} enemy={} outpost={} fortify={} chosen={} net={:+}",
                self.tick,
                self.players[player].name,
                ap_at_submission[player],
                counts.neutral,
                counts.enemy,
                counts.outpost,
                counts.fortify,
                if chosen.is_empty() {
                    "-".to_string()
                } else {
                    chosen.join(",")
                },
                net
            ));
        }
        if self.is_finished() {
            self.log(format!(
                "Finished. Scores (supplied fields): {}",
                self.players
                    .iter()
                    .map(|p| {
                        let player = self.player_index(p.name);
                        let supplied = self.score(player);
                        let total = self.total_fields(player);
                        format!(
                            "{} {} (unsupplied {}, total {})",
                            p.name,
                            supplied,
                            total - supplied,
                            total
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        if let Some(frames) = frames {
            frames.push(self.clone());
        }
    }

    fn player_index(&self, name: char) -> usize {
        self.players
            .iter()
            .position(|player| player.name == name)
            .unwrap_or(0)
    }

    fn produce(&mut self) {
        let mut production: BTreeMap<usize, u32> = BTreeMap::new();
        for (cell_index, network_id) in self.cell_network.iter().enumerate() {
            let Some(network_id) = network_id else {
                continue;
            };
            let Some(owner) = self.networks.get(network_id).map(|network| network.owner) else {
                continue;
            };
            let distance = self.base_distances[owner][cell_index];
            let amount = if self.cells[cell_index].base {
                2
            } else {
                1 + (distance / 4) as u32
            };
            *production.entry(*network_id).or_default() += amount;
            self.players[owner].produced += u64::from(amount);
        }
        for (network_id, amount) in production {
            if let Some(network) = self.networks.get_mut(&network_id) {
                network.stock = network.stock.saturating_add(amount);
            }
        }
    }

    #[cfg(test)]
    fn purchase_ap(&mut self, player: usize) {
        let (purchased, total_cost) = self.purchase_ap_up_to(player, MAX_AP);
        if purchased > 0 {
            self.log(format!(
                "{} bought {} AP for {} supply (AP {}/{})",
                self.players[player].name, purchased, total_cost, self.players[player].ap, MAX_AP
            ));
        }
    }

    fn purchase_ap_up_to(&mut self, player: usize, target_ap: u8) -> (usize, u32) {
        let mut purchased = 0usize;
        let mut total_cost = 0u32;

        while self.players[player].ap < target_ap && purchased < AP_PURCHASE_COSTS.len() {
            let cost = AP_PURCHASE_COSTS[purchased];
            let mut selected_network = None;
            let mut highest_stock = 0;
            for (&network_id, network) in &self.networks {
                if network.owner == player && network.stock >= cost && network.stock > highest_stock
                {
                    selected_network = Some(network_id);
                    highest_stock = network.stock;
                }
            }
            let Some(network_id) = selected_network else {
                break;
            };

            self.networks.get_mut(&network_id).unwrap().stock -= cost;
            self.players[player].ap += 1;
            self.players[player].spent += u64::from(cost);
            purchased += 1;
            total_cost += cost;
        }

        (purchased, total_cost)
    }

    fn plan_player_orders(&mut self, player: usize) -> Vec<Action> {
        let previous_tick_targets = self.last_tick_targets[player].clone();
        for desired_orders in (1..=MAX_AP).rev() {
            let mut forecast = self.clone();
            if desired_orders > forecast.players[player].ap {
                forecast.purchase_ap_up_to(player, desired_orders);
                if forecast.players[player].ap < desired_orders {
                    continue;
                }
            }

            let mut orders = Vec::new();
            let mut acted_targets = previous_tick_targets.clone();
            while orders.len() < usize::from(desired_orders) {
                let Some(action) = forecast.choose_action(player, &acted_targets) else {
                    break;
                };
                let network_id = forecast.cell_network[action.origin]
                    .expect("planned action origin is supplied");
                forecast.networks.get_mut(&network_id).unwrap().stock -= action.cost;
                acted_targets.insert(action.target);
                orders.push(action);
            }

            if orders.len() == usize::from(desired_orders) {
                if desired_orders > self.players[player].ap {
                    let (purchased, total_cost) = self.purchase_ap_up_to(player, desired_orders);
                    self.log(format!(
                        "{} bought {} AP for {} supply (AP {}/{})",
                        self.players[player].name,
                        purchased,
                        total_cost,
                        self.players[player].ap,
                        MAX_AP
                    ));
                }
                self.last_tick_targets[player] =
                    orders.iter().map(|action| action.target).collect();
                return orders;
            }
        }
        self.last_tick_targets[player].clear();
        Vec::new()
    }

    fn resolve_orders(&mut self, orders: Vec<(usize, Action)>) {
        for (player, action) in &orders {
            let network_id = self.cell_network[action.origin].expect("action origin is supplied");
            self.networks.get_mut(&network_id).unwrap().stock -= action.cost;
            self.players[*player].ap -= 1;
            self.players[*player].spent += u64::from(action.cost);
            if matches!(action.kind, ActionKind::Attack { .. }) {
                self.players[*player].attacks += 1;
            }
            self.last_action_sources.insert(action.origin);
            self.last_action_targets.insert(action.target);
        }

        let mut orders_by_target: BTreeMap<usize, Vec<(usize, Action)>> = BTreeMap::new();
        for order in orders {
            orders_by_target
                .entry(order.1.target)
                .or_default()
                .push(order);
        }

        let mut outcomes = Vec::new();
        for (target, target_orders) in orders_by_target {
            let original_owner = self.cells[target].owner;
            let original_defense = original_owner.map(|owner| self.defense_strength(target, owner));
            let mut captured_by = None;
            match original_owner {
                None => {
                    let claims: Vec<_> = target_orders
                        .iter()
                        .filter(|(_, action)| matches!(action.kind, ActionKind::Claim))
                        .map(|(player, _)| *player)
                        .collect();
                    if claims.len() == 1 {
                        captured_by = claims.first().copied();
                    } else if claims.len() > 1 {
                        self.log(format!(
                            "{} players contested neutral field {}; it remains neutral",
                            claims.len(),
                            self.label(target)
                        ));
                    }
                }
                Some(defender) => {
                    let attacks: Vec<_> = target_orders
                        .iter()
                        .filter_map(|(player, action)| match action.kind {
                            ActionKind::Attack { strength } => Some((*player, strength)),
                            _ => None,
                        })
                        .collect();
                    if !attacks.is_empty() {
                        let highest_bid =
                            attacks.iter().map(|(_, strength)| *strength).max().unwrap();
                        let leaders: Vec<_> = attacks
                            .iter()
                            .filter(|(_, strength)| *strength == highest_bid)
                            .map(|(player, _)| *player)
                            .collect();
                        let defense = original_defense.unwrap();
                        if highest_bid > defense && leaders.len() == 1 {
                            captured_by = leaders.first().copied();
                        }
                        for (attacker, strength) in attacks {
                            let result = if captured_by == Some(attacker) {
                                format!(
                                    "won against {}'s defense of {defense}",
                                    self.players[defender].name
                                )
                            } else if captured_by.is_some() {
                                format!(
                                    "lost to {}'s higher bid",
                                    self.players[captured_by.unwrap()].name
                                )
                            } else if strength < highest_bid {
                                format!("lost to {}'s higher bid", self.players[leaders[0]].name)
                            } else if leaders.len() > 1 {
                                format!(
                                    "tied the highest bid; {} held at {defense}",
                                    self.players[defender].name
                                )
                            } else {
                                format!(
                                    "did not beat {}'s defense of {defense}",
                                    self.players[defender].name
                                )
                            };
                            self.log(format!(
                                "{} bid strength {strength} at {}; {result}",
                                self.players[attacker].name,
                                self.label(target)
                            ));
                        }
                    }
                }
            }

            for (player, action) in target_orders {
                match action.kind {
                    ActionKind::Claim if captured_by == Some(player) => {
                        outcomes.push((player, action, original_owner, original_defense));
                    }
                    ActionKind::Attack { .. } if captured_by == Some(player) => {
                        outcomes.push((player, action, original_owner, original_defense));
                    }
                    ActionKind::BuildOutpost | ActionKind::Fortify
                        if captured_by.is_none() && original_owner == Some(player) =>
                    {
                        outcomes.push((player, action, original_owner, original_defense));
                    }
                    ActionKind::BuildOutpost | ActionKind::Fortify
                        if captured_by.is_some() && original_owner == Some(player) =>
                    {
                        self.log(format!(
                            "{}'s order at {} was disrupted by the capture",
                            self.players[player].name,
                            self.label(target)
                        ));
                    }
                    _ => {}
                }
            }
        }

        let mut network_changed = false;
        for (player, action, original_owner, original_defense) in outcomes {
            match action.kind {
                ActionKind::Claim => {
                    self.cells[action.target].owner = Some(player);
                    self.cells[action.target].unsupplied_ticks = 0;
                    self.players[player].fields_taken_by_discovery += 1;
                    self.log(format!(
                        "{} claimed {}",
                        self.players[player].name,
                        self.label(action.target)
                    ));
                    network_changed = true;
                }
                ActionKind::Attack { strength } => {
                    let defender =
                        original_owner.expect("attack target had an owner at order time");
                    let defense = original_defense.expect("owned targets have a defense strength");
                    if self.cells[action.target].outpost {
                        self.players[defender].outposts_lost += 1;
                    }
                    self.cells[action.target].owner = Some(player);
                    self.cells[action.target].unsupplied_ticks = 0;
                    self.cells[action.target].outpost = false;
                    self.cells[action.target].fortification = 0;
                    self.outpost_strain.remove(&action.target);
                    self.players[player].fields_won_in_combat += 1;
                    self.log(format!(
                        "{} took {} from {} ({} vs {})",
                        self.players[player].name,
                        self.label(action.target),
                        self.players[defender].name,
                        strength,
                        defense
                    ));
                    network_changed = true;
                }
                ActionKind::BuildOutpost => {
                    self.cells[action.target].outpost = true;
                    self.players[player].outposts_built += 1;
                    self.log(format!(
                        "{} built outpost at {}",
                        self.players[player].name,
                        self.label(action.target)
                    ));
                    network_changed = true;
                }
                ActionKind::Fortify => {
                    self.cells[action.target].fortification = 1;
                    self.log(format!(
                        "{} fortified {}",
                        self.players[player].name,
                        self.label(action.target)
                    ));
                }
            }
        }
        if network_changed {
            self.refresh_networks();
        }
    }

    fn distances_from(&self, source: usize) -> Vec<usize> {
        let mut distances = vec![usize::MAX; self.cells.len()];
        let mut queue = VecDeque::from([source]);
        distances[source] = 0;
        while let Some(cell) = queue.pop_front() {
            for neighbor in self.neighbors(cell) {
                if distances[neighbor] == usize::MAX {
                    distances[neighbor] = distances[cell] + 1;
                    queue.push_back(neighbor);
                }
            }
        }
        distances
    }

    fn choose_action(&self, player: usize, acted_targets: &HashSet<usize>) -> Option<Action> {
        let stance = self.players[player].stance;
        let mut attacks = Vec::new();
        let mut claims = Vec::new();
        for (origin, network_id) in self.cell_network.iter().enumerate() {
            if self.cells[origin].owner != Some(player) || network_id.is_none() {
                continue;
            }
            let stock = self.networks[&network_id.unwrap()].stock;
            for target in self.neighbors(origin) {
                match self.cells[target].owner {
                    None if stock >= 2 && !acted_targets.contains(&target) => claims.push(Action {
                        origin,
                        target,
                        kind: ActionKind::Claim,
                        cost: 2,
                    }),
                    Some(owner) if owner != player && !self.cells[target].base => {
                        if acted_targets.contains(&target) {
                            continue;
                        }
                        let defense = self.defense_strength(target, owner);
                        if let Some((strength, cost)) = Self::attack_cost(defense, stock, stance) {
                            attacks.push(Action {
                                origin,
                                target,
                                kind: ActionKind::Attack { strength },
                                cost,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let reconnect_distances = self.reconnection_distances(player);
        if let Some(action) = claims
            .iter()
            .chain(&attacks)
            .filter(|action| reconnect_distances[action.target] != usize::MAX)
            .min_by_key(|action| {
                (
                    reconnect_distances[action.target],
                    action.cost,
                    self.field_tiebreaker(player, action.target),
                    action.target,
                )
            })
        {
            return Some(*action);
        }

        if stance == Stance::BaseBuilder {
            if let Some(action) = self.outpost_action(player, acted_targets) {
                return Some(action);
            }
            if let Some(action) = self.fortify_action(player, acted_targets) {
                return Some(action);
            }
        }

        if stance == Stance::Aggro && !attacks.is_empty() {
            attacks.sort_by_key(|action| self.attack_priority(player, action));
            return attacks.into_iter().next();
        }

        if !claims.is_empty() {
            claims.sort_by_key(|action| {
                (
                    self.claim_priority(player, action.target),
                    self.field_tiebreaker(player, action.target),
                    action.target,
                )
            });
            return claims.into_iter().next();
        }

        if stance != Stance::Aggro && !attacks.is_empty() {
            attacks.sort_by_key(|action| self.attack_priority(player, action));
            return attacks.into_iter().next();
        }
        None
    }

    fn reconnection_distances(&self, player: usize) -> Vec<usize> {
        let mut distances = vec![usize::MAX; self.cells.len()];
        let mut queue = VecDeque::new();
        for (index, cell) in self.cells.iter().enumerate() {
            if cell.owner == Some(player) && self.cell_network[index].is_none() && !cell.wall {
                distances[index] = 0;
                queue.push_back(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            for neighbor in self.neighbors(index) {
                let cell = &self.cells[neighbor];
                if (cell.base && cell.owner != Some(player))
                    || (cell.owner == Some(player) && self.cell_network[neighbor].is_some())
                {
                    continue;
                }
                if distances[neighbor] == usize::MAX {
                    distances[neighbor] = distances[index] + 1;
                    queue.push_back(neighbor);
                }
            }
        }
        distances
    }

    fn attack_priority(&self, player: usize, action: &Action) -> (u8, u32, u64, usize) {
        (
            self.defense_strength(action.target, self.cells[action.target].owner.unwrap()),
            action.cost,
            self.field_tiebreaker(player, action.target),
            action.target,
        )
    }

    fn field_tiebreaker(&self, player: usize, target: usize) -> u64 {
        let (x, y) = Self::xy(target, self.board_width);
        let mut hash = 0x6a09e667f3bcc909_u64;
        for value in [
            self.simulation_no as u64,
            u64::from(self.tick),
            player as u64,
            x as u64,
            y as u64,
        ] {
            hash = hash.wrapping_add(value).wrapping_add(0x9e3779b97f4a7c15);
            hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
            hash ^= hash >> 31;
        }
        hash
    }

    fn claim_priority(&self, player: usize, target: usize) -> (usize, usize, usize) {
        let (x, y) = Self::xy(target, self.board_width);
        let edge_distance = x
            .min(y)
            .min(self.board_width - 1 - x)
            .min(self.board_height - 1 - y);
        let own_neighbors = self
            .neighbors(target)
            .into_iter()
            .filter(|&neighbor| self.cells[neighbor].owner == Some(player))
            .count();
        let enemy_neighbors = self
            .neighbors(target)
            .into_iter()
            .filter(|&neighbor| {
                self.cells[neighbor]
                    .owner
                    .is_some_and(|owner| owner != player)
            })
            .count();
        match self.players[player].stance {
            Stance::Explorer => (edge_distance, enemy_neighbors, own_neighbors),
            Stance::Aggro => (usize::MAX - enemy_neighbors, edge_distance, own_neighbors),
            Stance::BaseBuilder => {
                let (base_x, base_y) = Self::xy(self.players[player].base, self.board_width);
                let base_distance = x.abs_diff(base_x) + y.abs_diff(base_y);
                (usize::MAX - own_neighbors, base_distance, enemy_neighbors)
            }
        }
    }

    fn outpost_action(&self, player: usize, acted_targets: &HashSet<usize>) -> Option<Action> {
        let outpost_count = self
            .cells
            .iter()
            .filter(|cell| cell.owner == Some(player) && cell.outpost)
            .count();
        if outpost_count >= 2 {
            return None;
        }
        self.cell_network
            .iter()
            .enumerate()
            .filter(|(cell, network_id)| {
                network_id.is_some()
                    && !acted_targets.contains(cell)
                    && self.cells[*cell].owner == Some(player)
                    && !self.cells[*cell].base
                    && !self.cells[*cell].outpost
                    && self
                        .neighbors(*cell)
                        .iter()
                        .filter(|&&n| self.cells[n].owner == Some(player))
                        .count()
                        >= 3
            })
            .find_map(|(cell, network_id)| {
                let network_id = network_id.unwrap();
                (self.networks[&network_id].stock >= 15).then_some(Action {
                    origin: cell,
                    target: cell,
                    kind: ActionKind::BuildOutpost,
                    cost: 15,
                })
            })
    }

    fn fortify_action(&self, player: usize, acted_targets: &HashSet<usize>) -> Option<Action> {
        self.cell_network
            .iter()
            .enumerate()
            .filter(|(cell, network_id)| {
                network_id.is_some()
                    && !acted_targets.contains(cell)
                    && self.cells[*cell].owner == Some(player)
                    && self.cells[*cell].fortification == 0
                    && self.neighbors(*cell).iter().any(|&neighbor| {
                        self.cells[neighbor]
                            .owner
                            .is_some_and(|owner| owner != player)
                    })
            })
            .find_map(|(cell, network_id)| {
                let network_id = network_id.unwrap();
                (self.networks[&network_id].stock >= 3).then_some(Action {
                    origin: cell,
                    target: cell,
                    kind: ActionKind::Fortify,
                    cost: 3,
                })
            })
    }

    fn attack_cost(defense: u8, stock: u32, stance: Stance) -> Option<(u8, u32)> {
        let minimum_strength = defense.saturating_add(1).max(2);
        if minimum_strength > 6 {
            return None;
        }
        let strength = match stance {
            Stance::Aggro => {
                let preferred_strength = (minimum_strength + 1).min(6);
                if 5 + u32::from(preferred_strength - 2) * 2 <= stock {
                    preferred_strength
                } else {
                    (minimum_strength..=6)
                        .find(|strength| 5 + u32::from(*strength - 2) * 2 <= stock)?
                }
            }
            Stance::Explorer | Stance::BaseBuilder => {
                (minimum_strength..=6).find(|strength| 5 + u32::from(*strength - 2) * 2 <= stock)?
            }
        };
        let cost = 5 + u32::from(strength - 2) * 2;
        Some((strength, cost))
    }

    fn defense_strength(&self, cell: usize, owner: usize) -> u8 {
        let neighbors = self
            .neighbors(cell)
            .into_iter()
            .filter(|&neighbor| self.cells[neighbor].owner == Some(owner))
            .count()
            .min(2) as u8;
        (1 + neighbors + self.cells[cell].fortification + u8::from(self.cells[cell].base)).max(1)
    }

    #[cfg(test)]
    fn apply_action(&mut self, player: usize, action: Action) {
        self.last_action_sources.insert(action.origin);
        self.last_action_targets.insert(action.target);
        let network_id = self.cell_network[action.origin].expect("action origin is supplied");
        let network = self.networks.get_mut(&network_id).expect("network exists");
        network.stock -= action.cost;
        self.players[player].ap -= 1;
        self.players[player].spent += u64::from(action.cost);
        match action.kind {
            ActionKind::Claim => {
                self.cells[action.target].owner = Some(player);
                self.cells[action.target].unsupplied_ticks = 0;
                self.players[player].fields_taken_by_discovery += 1;
                self.log(format!(
                    "{} claimed {}",
                    self.players[player].name,
                    self.label(action.target)
                ));
                self.refresh_networks();
            }
            ActionKind::Attack { strength } => {
                let defender = self.cells[action.target]
                    .owner
                    .expect("attack target has an owner");
                let defense = self.defense_strength(action.target, defender);
                self.players[player].attacks += 1;
                if self.cells[action.target].outpost {
                    self.players[defender].outposts_lost += 1;
                }
                self.cells[action.target].owner = Some(player);
                self.cells[action.target].unsupplied_ticks = 0;
                self.cells[action.target].outpost = false;
                self.cells[action.target].fortification = 0;
                self.outpost_strain.remove(&action.target);
                self.players[player].fields_won_in_combat += 1;
                self.log(format!(
                    "{} took {} from {} ({} vs {})",
                    self.players[player].name,
                    self.label(action.target),
                    self.players[defender].name,
                    strength,
                    defense
                ));
                self.refresh_networks();
            }
            ActionKind::BuildOutpost => {
                self.cells[action.target].outpost = true;
                self.players[player].outposts_built += 1;
                self.log(format!(
                    "{} built outpost at {}",
                    self.players[player].name,
                    self.label(action.target)
                ));
                self.refresh_networks();
            }
            ActionKind::Fortify => {
                self.cells[action.target].fortification = 1;
                self.log(format!(
                    "{} fortified {}",
                    self.players[player].name,
                    self.label(action.target)
                ));
            }
        }
    }

    fn capture_surrounded_neutral_areas(&mut self) -> bool {
        let mut visited = vec![false; self.cells.len()];
        let mut captures: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

        for start in 0..self.cells.len() {
            if visited[start] || self.cells[start].wall || self.cells[start].owner.is_some() {
                continue;
            }

            let mut queue = VecDeque::from([start]);
            let mut area = Vec::new();
            let mut touches_edge = false;
            let mut boundary_owner = None;
            let mut closed = true;
            visited[start] = true;

            while let Some(cell) = queue.pop_front() {
                area.push(cell);
                let (x, y) = Self::xy(cell, self.board_width);
                touches_edge |=
                    x == 0 || y == 0 || x + 1 == self.board_width || y + 1 == self.board_height;

                for neighbor in self.adjacent_cells(cell) {
                    if self.cells[neighbor].wall {
                        continue;
                    }
                    if let Some(owner) = self.cells[neighbor].owner {
                        if boundary_owner.is_some_and(|boundary| boundary != owner) {
                            closed = false;
                        } else {
                            boundary_owner = Some(owner);
                        }
                    } else if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }

            if !touches_edge
                && closed
                && let Some(owner) = boundary_owner
            {
                captures.entry(owner).or_default().extend(area);
            }
        }

        if captures.is_empty() {
            return false;
        }

        for (owner, cells) in captures {
            let captured_count = cells.len();
            for cell in cells {
                self.cells[cell].owner = Some(owner);
                self.last_action_targets.insert(cell);
            }
            self.players[owner].fields_taken_by_discovery += captured_count as u32;
            self.log(format!(
                "{} encircled and claimed {} neutral fields",
                self.players[owner].name, captured_count
            ));
        }
        self.refresh_networks();
        true
    }

    fn label(&self, cell: usize) -> String {
        let (x, y) = Self::xy(cell, self.board_width);
        format!("({}, {})", x + 1, y + 1)
    }

    fn refresh_networks(&mut self) {
        let old_networks = self.networks.clone();
        let mut new_networks = BTreeMap::new();
        let mut new_cell_network = vec![None; self.cells.len()];

        for player in 0..self.players.len() {
            let mut sources: Vec<usize> = self
                .cells
                .iter()
                .enumerate()
                .filter_map(|(index, cell)| {
                    (cell.owner == Some(player) && (cell.base || cell.outpost)).then_some(index)
                })
                .collect();
            sources.sort_unstable();
            for source in sources {
                if new_cell_network[source].is_some() {
                    continue;
                }
                let network_id = source;
                let mut queue = VecDeque::from([source]);
                new_cell_network[source] = Some(network_id);
                let mut network_cells = Vec::new();
                while let Some(cell) = queue.pop_front() {
                    network_cells.push(cell);
                    for neighbor in self.neighbors(cell) {
                        if self.cells[neighbor].owner == Some(player)
                            && new_cell_network[neighbor].is_none()
                        {
                            new_cell_network[neighbor] = Some(network_id);
                            queue.push_back(neighbor);
                        }
                    }
                }
                new_networks.insert(
                    network_id,
                    Network {
                        owner: player,
                        stock: 0,
                        cells: network_cells,
                    },
                );
            }
        }

        for (old_id, old_network) in old_networks {
            let mut overlaps: BTreeMap<usize, usize> = BTreeMap::new();
            for cell in &old_network.cells {
                if let Some(new_id) = new_cell_network[*cell] {
                    *overlaps.entry(new_id).or_default() += 1;
                }
            }
            let total: usize = overlaps.values().sum();
            if total == 0 {
                continue;
            }
            let mut remaining = old_network.stock;
            let mut destinations: Vec<_> = overlaps.into_iter().collect();
            destinations.sort_by_key(|(id, _)| (*id != old_id, *id));
            for (position, (new_id, overlap)) in destinations.iter().enumerate() {
                let share = if position + 1 == destinations.len() {
                    remaining
                } else {
                    let share =
                        ((u64::from(old_network.stock) * *overlap as u64) / total as u64) as u32;
                    remaining -= share;
                    share
                };
                if let Some(network) = new_networks.get_mut(new_id) {
                    network.stock = network.stock.saturating_add(share);
                }
            }
        }

        self.networks = new_networks;
        self.cell_network = new_cell_network;
        for (cell, network) in self.cells.iter_mut().zip(&self.cell_network) {
            if network.is_some() || cell.owner.is_none() {
                cell.unsupplied_ticks = 0;
            }
        }
    }

    fn expire_unsupplied_fields(&mut self) {
        let mut expired = Vec::new();
        for (index, cell) in self.cells.iter_mut().enumerate() {
            if cell.owner.is_some()
                && !cell.base
                && !cell.wall
                && self.cell_network[index].is_none()
            {
                cell.unsupplied_ticks = cell.unsupplied_ticks.saturating_add(1);
                if cell.unsupplied_ticks >= UNSUPPLIED_TICK_LIMIT {
                    expired.push((index, cell.owner.unwrap()));
                    *cell = Cell::neutral();
                }
            } else {
                cell.unsupplied_ticks = 0;
            }
        }
        for (index, owner) in &expired {
            self.last_action_targets.insert(*index);
            self.outpost_strain.remove(index);
            self.log(format!(
                "{} lost {} after {} ticks without supply; field is neutral",
                self.players[*owner].name,
                self.label(*index),
                UNSUPPLIED_TICK_LIMIT
            ));
        }
        if !expired.is_empty() {
            self.refresh_networks();
        }
    }

    fn check_outposts(&mut self) -> bool {
        let outposts: Vec<_> = self
            .cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| (cell.outpost && !cell.base).then_some(index))
            .collect();
        let mut collapsed = Vec::new();
        for outpost in outposts {
            let owner = self.cells[outpost].owner.unwrap();
            let Some(network_id) = self.cell_network[outpost] else {
                continue;
            };
            if Some(network_id) == self.cell_network[self.players[owner].base] {
                self.outpost_strain.remove(&outpost);
                continue;
            }
            let size = self.networks[&network_id].cells.len();
            if size < 5 {
                let strain = self.outpost_strain.entry(outpost).or_default();
                *strain = strain.saturating_add(1);
                if *strain >= 3 {
                    collapsed.push(outpost);
                }
            } else {
                self.outpost_strain.remove(&outpost);
            }
        }
        let has_collapsed = !collapsed.is_empty();
        for outpost in collapsed {
            let owner = self.cells[outpost].owner.unwrap();
            self.last_action_targets.insert(outpost);
            self.cells[outpost].outpost = false;
            self.outpost_strain.remove(&outpost);
            self.players[owner].outposts_lost += 1;
            self.log(format!(
                "{}'s outpost at {} collapsed",
                self.players[owner].name,
                self.label(outpost)
            ));
            self.refresh_networks();
        }
        has_collapsed
    }
}

#[derive(Default)]
struct StanceTotals {
    games: u32,
    seats: u32,
    wins: u32,
    score: u64,
    total_fields: u64,
}

fn winning_stances(game: &Game, scores: &[usize]) -> BTreeSet<usize> {
    let high_score = scores.iter().copied().max().unwrap_or(0);
    game.players
        .iter()
        .enumerate()
        .filter_map(|(index, player)| {
            (scores[index] == high_score).then_some(player.stance.index())
        })
        .collect()
}

fn create_log_directory() -> Result<PathBuf, Box<dyn Error>> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let logs_directory = PathBuf::from("logs");
    fs::create_dir_all(&logs_directory)?;
    let directory = logs_directory.join(format!(
        "run-{}-{:09}-{}",
        timestamp.as_secs(),
        timestamp.subsec_nanos(),
        std::process::id()
    ));
    fs::create_dir(&directory)?;
    Ok(directory)
}

fn run_batch(games: usize, ticks: u32, map: &MapDefinition) -> Result<(), Box<dyn Error>> {
    let log_directory = create_log_directory()?;
    let mut totals: [StanceTotals; STANCE_COUNT] = std::array::from_fn(|_| StanceTotals::default());
    for simulation in 0..games {
        let mut game = Game::from_map(simulation, ticks, map.clone());
        while !game.is_finished() {
            game.tick();
        }
        game.write_logs(&log_directory)?;
        let scores: Vec<_> = (0..PLAYER_COUNT).map(|player| game.score(player)).collect();
        let present_stances: BTreeSet<_> = game
            .players
            .iter()
            .map(|player| player.stance.index())
            .collect();
        let winning_stances = winning_stances(&game, &scores);
        for (index, player) in game.players.iter().enumerate() {
            let result = &mut totals[player.stance.index()];
            result.score += scores[index] as u64;
            result.total_fields += game.total_fields(index) as u64;
            result.seats += 1;
        }
        for stance in present_stances {
            totals[stance].games += 1;
        }
        for stance in winning_stances {
            totals[stance].wins += 1;
        }
    }
    println!(
        "{} deterministic games x {} ticks on {} ({}x{})",
        games, ticks, map.name, map.width, map.height
    );
    println!(
        "{:<14} {:>8} {:>8} {:>14} {:>16} {:>12}",
        "Stance", "Games", "Wins", "Avg supplied", "Avg unsupplied", "Avg total"
    );
    for (index, stance) in STANCE_NAMES.iter().enumerate() {
        let result = &totals[index];
        let average = if result.seats == 0 {
            0.0
        } else {
            result.score as f64 / result.seats as f64
        };
        let total_average = if result.seats == 0 {
            0.0
        } else {
            result.total_fields as f64 / result.seats as f64
        };
        println!(
            "{:<14} {:>8} {:>8} {:>14.2} {:>16.2} {:>12.2}",
            stance,
            result.games,
            result.wins,
            average,
            total_average - average,
            total_average
        );
    }
    println!(
        "Supplied fields decide the winner. Ties count as a win for each stance tied for first."
    );
    println!("Game logs saved in {}", log_directory.display());
    Ok(())
}

fn parse_args() -> Result<Options, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mut batch = None;
    let mut ticks = DEFAULT_TICKS;
    let mut frame_delay_ms = DEFAULT_FRAME_DELAY_MS;
    let mut generated_size = None;
    let mut map_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" => {
                let value = args.next().ok_or("--batch requires a game count")?;
                batch = Some(value.parse::<usize>()?);
                if batch == Some(0) {
                    return Err("--batch must be greater than zero".into());
                }
            }
            "--ticks" => {
                let value = args.next().ok_or("--ticks requires a tick count")?;
                ticks = value.parse::<u32>()?;
                if ticks == 0 {
                    return Err("--ticks must be greater than zero".into());
                }
            }
            "--delay-ms" => {
                let value = args.next().ok_or("--delay-ms requires a duration")?;
                frame_delay_ms = value.parse::<u64>()?;
                if frame_delay_ms == 0 {
                    return Err("--delay-ms must be greater than zero".into());
                }
            }
            "--generated-size" => {
                let value = args.next().ok_or("--generated-size requires a size")?;
                let size = value.parse::<usize>()?;
                if !is_valid_board_size(size) {
                    return Err(format!(
                        "--generated-size must be an odd number from {MIN_BOARD_SIZE} to {MAX_BOARD_SIZE}"
                    )
                    .into());
                }
                generated_size = Some(size);
            }
            "--board-size" => {
                return Err(
                    "the map file defines board size; use --generated-size N for a generated map"
                        .into(),
                );
            }
            "--map" => {
                let value = args.next().ok_or("--map requires a file path")?;
                map_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: borderlands [--ticks N] [--map FILE | --generated-size N] [--delay-ms N] [--batch N]"
                );
                println!(
                    "Launch the Bevy GUI; --map loads a text map, --generated-size generates one."
                );
                println!("Use --batch N for non-visual stance matchup tests.");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}; use --help").into()),
        }
    }
    if generated_size.is_some() && map_path.is_some() {
        return Err("--map and --generated-size cannot be used together".into());
    }
    Ok(Options {
        batch,
        ticks,
        frame_delay_ms,
        generated_size,
        map_path,
    })
}

fn load_selected_map(options: &Options) -> Result<MapDefinition, Box<dyn Error>> {
    let source = if let Some(path) = &options.map_path {
        fs::read_to_string(path)?
    } else if let Some(size) = options.generated_size {
        let mut map = MapDefinition::generated(size);
        map.name = format!("generated {size}x{size}");
        return Ok(map);
    } else {
        let mut map = MapDefinition::parse(DEFAULT_MAP)
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
        map.name = "maps/crossroads.map".to_string();
        return Ok(map);
    };
    let mut map = MapDefinition::parse(&source)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
    map.name = options
        .map_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "text map".to_string());
    Ok(map)
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = parse_args()?;
    let map = load_selected_map(&options)?;
    if let Some(games) = options.batch {
        run_batch(games, options.ticks, &map)?;
    } else {
        gui::run_gui(
            options.ticks,
            Duration::from_millis(options.frame_delay_ms),
            map,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_generated_maps_can_be_loaded_by_the_game() {
        for size in [11, 35, 101] {
            let generated = borderlands::map_generator::generate(size, "Grænseland").unwrap();
            let map = MapDefinition::parse(&generated.text).unwrap();
            assert_eq!((map.width, map.height), (size, size));
            let game = Game::from_map(0, 10, map);
            for player in 0..PLAYER_COUNT {
                assert!(game.cell_network[game.players[player].base].is_some());
            }
        }
    }

    fn tiebreaker_game() -> Game {
        let map = MapDefinition::parse(&format!(
            "A.........B\n{}C.........D",
            "...........\n".repeat(9)
        ))
        .unwrap();
        Game::from_map(0, 40, map)
    }

    #[test]
    fn unsupplied_fields_expire_on_the_fifth_tick_and_reset_when_reconnected() {
        let mut game = tiebreaker_game();
        let index = Game::index(2, 0, game.board_width);
        let bridge = Game::index(1, 0, game.board_width);
        game.cells[index].owner = Some(0);
        game.cells[index].fortification = 1;
        game.refresh_networks();
        for ticks in 1..=4 {
            game.expire_unsupplied_fields();
            assert_eq!(game.cells[index].owner, Some(0));
            assert_eq!(game.cells[index].unsupplied_ticks, ticks);
        }
        game.cells[bridge].owner = Some(0);
        game.refresh_networks();
        assert_eq!(game.cells[index].unsupplied_ticks, 0);
        game.cells[bridge].owner = None;
        game.refresh_networks();
        for _ in 0..4 {
            game.expire_unsupplied_fields();
        }
        assert_eq!(game.cells[index].owner, Some(0));
        game.expire_unsupplied_fields();
        assert_eq!(game.cells[index], Cell::neutral());
        assert!(
            game.activity_log
                .last()
                .unwrap()
                .contains("field is neutral")
        );
        assert_eq!(game.cells[game.players[0].base].owner, Some(0));
    }

    #[test]
    fn disconnected_outpost_fields_start_expiring_after_outpost_collapse() {
        let mut game = tiebreaker_game();
        let index = Game::index(5, 5, game.board_width);
        game.cells[index].owner = Some(0);
        game.cells[index].outpost = true;
        game.refresh_networks();
        for _ in 0..2 {
            game.check_outposts();
            game.expire_unsupplied_fields();
            assert_eq!(game.cells[index].unsupplied_ticks, 0);
        }
        game.check_outposts();
        game.expire_unsupplied_fields();
        assert!(!game.cells[index].outpost);
        assert_eq!(game.cells[index].unsupplied_ticks, 1);
        for _ in 0..4 {
            game.expire_unsupplied_fields();
        }
        assert_eq!(game.cells[index], Cell::neutral());
    }

    #[test]
    fn actual_ticks_neutralize_an_unreachable_field_after_exactly_five_ticks() {
        let mut game = tiebreaker_game();
        let index = Game::index(5, 5, game.board_width);
        for neighbor in game.neighbors(index) {
            game.cells[neighbor].wall = true;
        }
        game.cells[index].owner = Some(0);
        game.cells[index].fortification = 1;
        game.refresh_networks();
        for ticks in 1..5 {
            game.tick();
            assert_eq!(game.cells[index].owner, Some(0));
            assert_eq!(game.cells[index].unsupplied_ticks, ticks);
        }
        game.tick();
        assert_eq!(game.tick, 5);
        assert_eq!(game.cells[index], Cell::neutral());
        assert!(
            game.activity_log
                .iter()
                .any(|entry| entry.starts_with("T005") && entry.contains("field is neutral"))
        );
    }

    #[test]
    fn all_stances_prioritize_reconnecting_via_claims_or_attacks() {
        for stance in [Stance::Explorer, Stance::Aggro, Stance::BaseBuilder] {
            for enemy_bridge in [false, true] {
                let mut game = tiebreaker_game();
                game.players[0].stance = stance;
                let bridge = Game::index(1, 0, game.board_width);
                let isolated = Game::index(2, 0, game.board_width);
                game.cells[isolated].owner = Some(0);
                game.cells[isolated].unsupplied_ticks = 4;
                if enemy_bridge {
                    game.cells[bridge].owner = Some(1);
                }
                game.refresh_networks();
                let network = game.cell_network[game.players[0].base].unwrap();
                game.networks.get_mut(&network).unwrap().stock = 100;
                let action = game.choose_action(0, &HashSet::new()).unwrap();
                assert_eq!(action.target, bridge);
                assert_eq!(
                    matches!(action.kind, ActionKind::Attack { .. }),
                    enemy_bridge
                );
                game.resolve_orders(vec![(0, action)]);
                game.expire_unsupplied_fields();
                assert_eq!(game.cells[isolated].owner, Some(0));
                assert_eq!(game.cells[isolated].unsupplied_ticks, 0);
                assert!(game.cell_network[isolated].is_some());
            }
        }
    }

    #[test]
    fn reconnection_routes_can_span_multiple_ticks_and_do_not_cross_walls() {
        let mut game = tiebreaker_game();
        let isolated = Game::index(3, 0, game.board_width);
        game.cells[isolated].owner = Some(0);
        game.refresh_networks();
        for x in 1..=2 {
            let action = game.choose_action(0, &HashSet::new()).unwrap();
            assert_eq!(action.target, Game::index(x, 0, game.board_width));
            game.resolve_orders(vec![(0, action)]);
        }
        assert!(game.cell_network[isolated].is_some());
        let mut blocked = tiebreaker_game();
        for x in 0..blocked.board_width {
            let index = Game::index(x, 2, blocked.board_width);
            blocked.cells[index].wall = true;
        }
        let isolated = Game::index(3, 3, blocked.board_width);
        blocked.cells[isolated].owner = Some(0);
        blocked.refresh_networks();
        let distances = blocked.reconnection_distances(0);
        assert_eq!(
            distances[Game::index(1, 0, blocked.board_width)],
            usize::MAX
        );
    }

    #[test]
    fn equally_good_claims_vary_deterministically_for_all_stances() {
        let mut game = tiebreaker_game();
        let east = Game::index(1, 0, game.board_width);
        let south = Game::index(0, 1, game.board_width);
        for stance in [Stance::Explorer, Stance::Aggro, Stance::BaseBuilder] {
            game.players[0].stance = stance;
            assert_eq!(game.claim_priority(0, east), game.claim_priority(0, south));
            let mut chosen = HashSet::new();
            for tick in 0..32 {
                game.tick = tick;
                let expected = [east, south]
                    .into_iter()
                    .min_by_key(|&target| (game.field_tiebreaker(0, target), target))
                    .unwrap();
                for _ in 0..2 {
                    let action = game.choose_action(0, &HashSet::new()).unwrap();
                    assert!(matches!(action.kind, ActionKind::Claim));
                    assert_eq!(action.target, expected);
                }
                chosen.insert(expected);
            }
            assert_eq!(chosen, HashSet::from([east, south]));
        }
    }

    #[test]
    fn claim_variation_never_overrides_strategic_priority() {
        let mut game = tiebreaker_game();
        let east = Game::index(1, 0, game.board_width);
        let south = Game::index(0, 1, game.board_width);
        game.cells[Game::index(2, 0, game.board_width)].owner = Some(1);
        for stance in [Stance::Explorer, Stance::Aggro, Stance::BaseBuilder] {
            game.players[0].stance = stance;
            let expected = if stance == Stance::Aggro { east } else { south };
            let other = if expected == east { south } else { east };
            assert!(game.claim_priority(0, expected) < game.claim_priority(0, other));
            for tick in 0..32 {
                game.tick = tick;
                assert_eq!(
                    game.choose_action(0, &HashSet::new()).unwrap().target,
                    expected
                );
            }
        }
    }

    #[test]
    fn attack_variation_only_breaks_equal_defense_and_cost() {
        let mut game = tiebreaker_game();
        let east = Game::index(1, 0, game.board_width);
        let south = Game::index(0, 1, game.board_width);
        game.cells[east].owner = Some(1);
        game.cells[south].owner = Some(1);
        game.refresh_networks();
        let network = game.cell_network[game.players[0].base].unwrap();
        game.networks.get_mut(&network).unwrap().stock = 100;
        let acted_targets = HashSet::from([game.players[0].base]);
        for stance in [Stance::Explorer, Stance::Aggro, Stance::BaseBuilder] {
            game.players[0].stance = stance;
            let mut chosen = HashSet::new();
            for tick in 0..32 {
                game.tick = tick;
                let expected = [east, south]
                    .into_iter()
                    .min_by_key(|&target| (game.field_tiebreaker(0, target), target))
                    .unwrap();
                let action = game.choose_action(0, &acted_targets).unwrap();
                assert!(matches!(action.kind, ActionKind::Attack { .. }));
                assert_eq!(action.target, expected);
                chosen.insert(expected);
            }
            assert_eq!(chosen, HashSet::from([east, south]));
            game.cells[east].fortification = 1;
            for tick in 0..32 {
                game.tick = tick;
                assert_eq!(game.choose_action(0, &acted_targets).unwrap().target, south);
            }
            game.cells[east].fortification = 0;
            let cheaper = Action {
                origin: game.players[0].base,
                target: east,
                kind: ActionKind::Attack { strength: 2 },
                cost: 5,
            };
            let dearer = Action {
                target: south,
                cost: 7,
                ..cheaper
            };
            for tick in 0..32 {
                game.tick = tick;
                assert!(game.attack_priority(0, &cheaper) < game.attack_priority(0, &dearer));
            }
        }
    }

    #[test]
    fn field_tiebreaker_uses_seed_tick_player_and_coordinates() {
        let mut game = tiebreaker_game();
        let east = Game::index(1, 0, game.board_width);
        let south = Game::index(0, 1, game.board_width);
        let baseline = game.field_tiebreaker(0, east);
        assert_eq!(baseline, game.clone().field_tiebreaker(0, east));
        assert_ne!(baseline, game.field_tiebreaker(1, east));
        assert_ne!(baseline, game.field_tiebreaker(0, south));
        game.tick = 1;
        assert_ne!(baseline, game.field_tiebreaker(0, east));
        game.tick = 0;
        game.simulation_no = 1;
        assert_ne!(baseline, game.field_tiebreaker(0, east));
        game.simulation_no = 0;
        let player_rankings: HashSet<_> = (0..PLAYER_COUNT)
            .map(|player| {
                game.field_tiebreaker(player, east) < game.field_tiebreaker(player, south)
            })
            .collect();
        assert_eq!(player_rankings.len(), 2);
        let seed_rankings: HashSet<_> = (0..16)
            .map(|seed| {
                game.simulation_no = seed;
                game.field_tiebreaker(0, east) < game.field_tiebreaker(0, south)
            })
            .collect();
        assert_eq!(seed_rankings.len(), 2);
    }

    #[test]
    fn repeated_games_have_identical_choices_and_activity() {
        let mut first = tiebreaker_game();
        let mut second = tiebreaker_game();
        while !first.is_finished() {
            first.tick();
            second.tick();
        }
        assert_eq!(first.cells, second.cells);
        assert_eq!(first.choice_log, second.choice_log);
        assert_eq!(first.activity_log, second.activity_log);
    }

    #[test]
    fn walls_block_neighbors_and_supply() {
        let mut game = Game::new(0, 10);
        let home = game.players[0].base;
        let wall = Game::index(5, 2, DEFAULT_BOARD_SIZE);
        let isolated = Game::index(1, 3, DEFAULT_BOARD_SIZE);
        for x in 0..DEFAULT_BOARD_SIZE {
            game.cells[Game::index(x, 2, DEFAULT_BOARD_SIZE)].wall = true;
        }
        game.cells[isolated].owner = Some(0);
        game.refresh_networks();
        assert!(game.cells[wall].wall);
        assert!(!game.neighbors(home).contains(&wall));
        assert!(game.cell_network[home].is_some());
        assert!(game.cell_network[isolated].is_none());
    }

    #[test]
    fn enclosed_neutral_area_is_captured_and_connected_to_supply() {
        let mut game = Game::with_board_size(0, 10, 25);
        for y in 1..=4 {
            for x in 1..=4 {
                if x == 1 || x == 4 || y == 1 || y == 4 {
                    game.cells[Game::index(x, y, 25)].owner = Some(0);
                }
            }
        }
        let area: Vec<_> = (2..=3)
            .flat_map(|y| (2..=3).map(move |x| Game::index(x, y, 25)))
            .collect();
        let fields_taken_before = game.players[0].fields_taken_by_discovery;

        assert!(game.capture_surrounded_neutral_areas());
        assert!(area.iter().all(|&cell| game.cells[cell].owner == Some(0)));
        assert_eq!(
            game.players[0].fields_taken_by_discovery - fields_taken_before,
            4
        );
        assert_eq!(game.players[0].fields_won_in_combat, 0);
        let home_network = game.cell_network[game.players[0].base];
        assert!(
            area.iter()
                .all(|&cell| game.cell_network[cell] == home_network)
        );
        assert!(
            area.iter()
                .all(|cell| game.last_action_targets.contains(cell))
        );
    }

    #[test]
    fn neutral_area_open_to_edge_is_not_captured() {
        let mut game = Game::new(0, 10);
        let edge = Game::index(0, 5, DEFAULT_BOARD_SIZE);
        for cell in [
            Game::index(1, 5, DEFAULT_BOARD_SIZE),
            Game::index(0, 4, DEFAULT_BOARD_SIZE),
            Game::index(0, 6, DEFAULT_BOARD_SIZE),
        ] {
            game.cells[cell].owner = Some(0);
        }

        assert!(!game.capture_surrounded_neutral_areas());
        assert_eq!(game.cells[edge].owner, None);
    }

    #[test]
    fn mixed_player_boundary_does_not_capture_neutral_or_enemy_fields() {
        let mut game = Game::new(0, 10);
        let target = Game::index(5, 5, DEFAULT_BOARD_SIZE);
        let boundary = [
            Game::index(4, 5, DEFAULT_BOARD_SIZE),
            Game::index(6, 5, DEFAULT_BOARD_SIZE),
            Game::index(5, 4, DEFAULT_BOARD_SIZE),
            Game::index(5, 6, DEFAULT_BOARD_SIZE),
        ];
        for &cell in &boundary[..3] {
            game.cells[cell].owner = Some(0);
        }
        game.cells[boundary[3]].owner = Some(1);

        assert!(!game.capture_surrounded_neutral_areas());
        assert_eq!(game.cells[target].owner, None);
        assert_eq!(game.cells[boundary[3]].owner, Some(1));
    }

    #[test]
    fn walls_can_complete_an_encirclement() {
        let mut game = Game::new(0, 10);
        let target = Game::index(2, 2, DEFAULT_BOARD_SIZE);
        let wall = Game::index(2, 1, DEFAULT_BOARD_SIZE);
        game.cells[wall].wall = true;
        for cell in [
            Game::index(1, 2, DEFAULT_BOARD_SIZE),
            Game::index(3, 2, DEFAULT_BOARD_SIZE),
            Game::index(2, 3, DEFAULT_BOARD_SIZE),
        ] {
            game.cells[cell].owner = Some(0);
        }

        assert!(game.capture_surrounded_neutral_areas());
        assert_eq!(game.cells[target].owner, Some(0));
        assert!(game.cells[wall].wall);
    }

    #[test]
    fn players_plan_orders_from_the_same_tick_snapshot() {
        let mut game = Game::with_board_size(0, 10, 25);
        for cell in &mut game.cells {
            cell.wall = true;
        }

        let paths = [
            [(1, 0), (2, 0)],
            [(23, 0), (22, 0)],
            [(1, 24), (2, 24)],
            [(23, 24), (22, 24)],
        ];
        for (player, path) in paths.iter().enumerate() {
            game.cells[game.players[player].base].wall = false;
            for &(x, y) in path {
                game.cells[Game::index(x, y, 25)].wall = false;
            }
        }
        game.refresh_networks();

        let frames = game.tick_frames();
        assert_eq!(frames.len(), 1);
        for (player, path) in paths.iter().enumerate() {
            let first_target = Game::index(path[0].0, path[0].1, 25);
            let second_target = Game::index(path[1].0, path[1].1, 25);
            assert_eq!(frames[0].score(player), 2);
            assert!(game.last_tick_targets[player].contains(&first_target));
            assert!(!game.last_tick_targets[player].contains(&second_target));
        }
        assert!(game.players.iter().all(|player| {
            player.fields_taken_by_discovery == 1 && player.fields_won_in_combat == 0
        }));
        assert_eq!(game.choice_log.len(), PLAYER_COUNT);
        assert!(game.choice_log.iter().all(|entry| entry.contains("AP=6")
            && entry.contains("chosen=claim@")
            && entry.contains("net=+1")));
    }

    #[test]
    fn tied_claims_spend_supply_and_leave_the_neutral_field_unclaimed() {
        let mut game = Game::new(0, 10);
        let target = Game::index(5, 5, DEFAULT_BOARD_SIZE);
        let origins = [
            Game::index(4, 5, DEFAULT_BOARD_SIZE),
            Game::index(6, 5, DEFAULT_BOARD_SIZE),
        ];
        let mut orders = Vec::new();
        for (player, origin) in [(0, origins[0]), (1, origins[1])] {
            game.cells[origin].owner = Some(player);
            game.cell_network[origin] = Some(origin);
            game.networks.insert(
                origin,
                Network {
                    owner: player,
                    stock: 10,
                    cells: vec![origin],
                },
            );
            orders.push((
                player,
                Action {
                    origin,
                    target,
                    kind: ActionKind::Claim,
                    cost: 2,
                },
            ));
        }

        game.resolve_orders(orders);

        assert_eq!(game.cells[target].owner, None);
        assert_eq!(game.players[0].ap, MAX_AP - 1);
        assert_eq!(game.players[1].ap, MAX_AP - 1);
        assert_eq!(game.players[0].spent, 2);
        assert_eq!(game.players[1].spent, 2);
        assert_eq!(game.networks[&origins[0]].stock, 8);
        assert_eq!(game.networks[&origins[1]].stock, 8);
    }

    #[test]
    fn highest_unique_attack_bid_wins_and_losing_bids_are_spent() {
        let mut game = Game::new(0, 10);
        let target = Game::index(5, 5, DEFAULT_BOARD_SIZE);
        game.cells[target].owner = Some(1);
        let orders = [
            (0, Game::index(4, 5, DEFAULT_BOARD_SIZE), 3, 7),
            (2, Game::index(6, 5, DEFAULT_BOARD_SIZE), 4, 9),
        ]
        .map(|(player, origin, strength, cost)| {
            game.cells[origin].owner = Some(player);
            game.cell_network[origin] = Some(origin);
            game.networks.insert(
                origin,
                Network {
                    owner: player,
                    stock: 20,
                    cells: vec![origin],
                },
            );
            (
                player,
                Action {
                    origin,
                    target,
                    kind: ActionKind::Attack { strength },
                    cost,
                },
            )
        });

        game.resolve_orders(orders.into());

        assert_eq!(game.cells[target].owner, Some(2));
        assert_eq!(game.players[0].spent, 7);
        assert_eq!(game.players[2].spent, 9);
        assert_eq!(game.players[0].ap, MAX_AP - 1);
        assert_eq!(game.players[2].ap, MAX_AP - 1);
        assert_eq!(game.players[2].fields_won_in_combat, 1);
        assert_eq!(game.players[2].fields_taken_by_discovery, 0);
        assert_eq!(game.players[0].fields_won_in_combat, 0);
    }

    #[test]
    fn tied_attack_bids_leave_the_field_with_its_defender() {
        let mut game = Game::new(0, 10);
        let target = Game::index(5, 5, DEFAULT_BOARD_SIZE);
        game.cells[target].owner = Some(1);
        let orders = [
            (0, Game::index(4, 5, DEFAULT_BOARD_SIZE)),
            (2, Game::index(6, 5, DEFAULT_BOARD_SIZE)),
        ]
        .map(|(player, origin)| {
            game.cells[origin].owner = Some(player);
            game.cell_network[origin] = Some(origin);
            game.networks.insert(
                origin,
                Network {
                    owner: player,
                    stock: 20,
                    cells: vec![origin],
                },
            );
            (
                player,
                Action {
                    origin,
                    target,
                    kind: ActionKind::Attack { strength: 3 },
                    cost: 7,
                },
            )
        });

        game.resolve_orders(orders.into());

        assert_eq!(game.cells[target].owner, Some(1));
        assert_eq!(game.players[0].spent, 7);
        assert_eq!(game.players[2].spent, 7);
        assert_eq!(game.players[0].fields_won_in_combat, 0);
        assert_eq!(game.players[2].fields_won_in_combat, 0);
    }

    #[test]
    fn bots_buy_ap_only_for_the_orders_they_can_fund() {
        let mut game = Game::new(0, 2);
        for cell in &mut game.cells {
            cell.wall = true;
        }
        let base = game.players[0].base;
        let (base_x, base_y) = Game::xy(base, DEFAULT_BOARD_SIZE);
        let targets = [
            Game::index(base_x + 1, base_y, DEFAULT_BOARD_SIZE),
            Game::index(base_x, base_y + 1, DEFAULT_BOARD_SIZE),
        ];
        game.cells[base].wall = false;
        for target in targets {
            game.cells[target].wall = false;
        }
        game.refresh_networks();
        let network_id = game.cell_network[base].unwrap();
        game.networks.get_mut(&network_id).unwrap().stock = 28;
        game.players[0].ap = 0;

        game.tick();

        assert_eq!(game.players[0].fields_taken_by_discovery, 2);
        assert_eq!(game.players[0].fields_won_in_combat, 0);
        assert_eq!(game.players[0].ap, 0);
        assert_eq!(game.networks[&game.cell_network[base].unwrap()].stock, 6);
        assert!(
            game.log
                .iter()
                .any(|entry| entry.contains("bought 1 AP for 20 supply"))
        );
    }

    #[test]
    fn player_does_not_choose_a_field_already_used_this_tick() {
        let mut game = Game::new(0, 10);
        game.players[0].stance = Stance::Aggro;
        for x in 2..=8 {
            game.cells[Game::index(x, 1, DEFAULT_BOARD_SIZE)].owner = Some(0);
        }
        let already_attacked = Game::index(7, 0, DEFAULT_BOARD_SIZE);
        let alternate = Game::index(8, 0, DEFAULT_BOARD_SIZE);
        game.cells[already_attacked].owner = Some(1);
        game.cells[alternate].owner = Some(1);
        game.refresh_networks();
        let network_id = game.cell_network[game.players[0].base].unwrap();
        game.networks.get_mut(&network_id).unwrap().stock = 100;

        let action = game
            .choose_action(0, &HashSet::from([already_attacked]))
            .expect("another enemy field can be attacked");
        assert_eq!(action.target, alternate);
        assert!(matches!(action.kind, ActionKind::Attack { .. }));
    }

    #[test]
    fn player_cannot_claim_a_field_already_used_this_tick() {
        let mut game = Game::new(0, 10);
        let base = game.players[0].base;
        let mut available_target = None;
        for neighbor in game.neighbors(base) {
            if available_target.is_none() {
                available_target = Some(neighbor);
            } else {
                game.cells[neighbor].wall = true;
            }
        }
        let target = available_target.unwrap();
        game.refresh_networks();
        let first = game
            .choose_action(0, &HashSet::new())
            .expect("one neutral field is available");
        assert_eq!(first.target, target);
        assert!(matches!(first.kind, ActionKind::Claim));
        assert!(game.choose_action(0, &HashSet::from([target])).is_none());
        assert_eq!(game.players[0].ap, MAX_AP);
        assert_eq!(
            game.networks[&game.cell_network[game.players[0].base].unwrap()].stock,
            12
        );
    }

    #[test]
    fn player_cannot_use_a_field_used_in_the_previous_tick() {
        let mut game = Game::new(0, 10);
        for cell in &mut game.cells {
            cell.wall = true;
        }
        for player in 0..PLAYER_COUNT {
            game.cells[game.players[player].base].wall = false;
        }
        let base = game.players[0].base;
        let target = game
            .adjacent_cells(base)
            .into_iter()
            .next()
            .expect("base has an adjacent field");
        game.cells[target].wall = false;
        game.refresh_networks();

        game.tick();
        assert_eq!(game.cells[target].owner, Some(0));
        assert!(game.last_tick_targets[0].contains(&target));

        game.cells[target].owner = None;
        game.refresh_networks();
        game.tick();
        assert_eq!(game.cells[target].owner, None);
        assert!(game.last_tick_targets[0].is_empty());
        assert_eq!(
            game.choose_action(0, &game.last_tick_targets[0])
                .expect("a field becomes available again after one tick without using it")
                .target,
            target
        );
    }

    #[test]
    fn base_builder_cannot_reuse_a_field_for_fortification_or_outpost() {
        let mut game = Game::new(0, 10);
        game.players[0].stance = Stance::BaseBuilder;
        let fortification_target = Game::index(2, 1, DEFAULT_BOARD_SIZE);
        let hostile_neighbor = Game::index(3, 1, DEFAULT_BOARD_SIZE);
        game.cells[fortification_target].owner = Some(0);
        game.cells[hostile_neighbor].owner = Some(1);

        let outpost_target = Game::index(2, 2, DEFAULT_BOARD_SIZE);
        for cell in [
            Game::index(1, 2, DEFAULT_BOARD_SIZE),
            Game::index(2, 3, DEFAULT_BOARD_SIZE),
            Game::index(3, 2, DEFAULT_BOARD_SIZE),
        ] {
            game.cells[cell].owner = Some(0);
        }
        game.cells[outpost_target].owner = Some(0);
        game.refresh_networks();
        let network_id = game.cell_network[game.players[0].base].unwrap();
        game.networks.get_mut(&network_id).unwrap().stock = 100;

        let fortify = game
            .fortify_action(0, &HashSet::new())
            .expect("frontline field can be fortified");
        assert_eq!(fortify.target, fortification_target);
        let next_fortify = game
            .fortify_action(0, &HashSet::from([fortification_target]))
            .expect("another eligible field may be fortified");
        assert_ne!(next_fortify.target, fortification_target);

        let build = game
            .outpost_action(0, &HashSet::new())
            .expect("eligible field can host an outpost");
        assert_eq!(build.target, outpost_target);
        assert!(
            game.outpost_action(0, &HashSet::from([outpost_target]))
                .is_none()
        );
    }

    #[test]
    fn actioned_field_limit_is_per_player() {
        let mut game = Game::new(0, 10);
        game.players[0].stance = Stance::Aggro;
        game.players[1].stance = Stance::Aggro;
        let target = Game::index(5, 1, DEFAULT_BOARD_SIZE);
        game.cells[target].owner = Some(2);
        for x in 2..=4 {
            game.cells[Game::index(x, 1, DEFAULT_BOARD_SIZE)].owner = Some(0);
        }
        for x in 6..=8 {
            game.cells[Game::index(x, 1, DEFAULT_BOARD_SIZE)].owner = Some(1);
        }
        game.refresh_networks();
        for player in 0..2 {
            let network_id = game.cell_network[game.players[player].base].unwrap();
            game.networks.get_mut(&network_id).unwrap().stock = 100;
        }

        let mut acted_targets = vec![HashSet::new(); PLAYER_COUNT];
        let first_action = game
            .choose_action(0, &acted_targets[0])
            .expect("player 0 can attack the shared target");
        assert_eq!(first_action.target, target);
        game.apply_action(0, first_action);
        acted_targets[0].insert(target);

        assert!(acted_targets[0].contains(&target));
        assert!(!acted_targets[1].contains(&target));
        let second_action = game
            .choose_action(1, &acted_targets[1])
            .expect("player 1 has an independent per-tick action set");
        assert_eq!(second_action.target, target);
        assert!(matches!(second_action.kind, ActionKind::Attack { .. }));
    }

    #[test]
    fn equal_attack_and_defense_favors_defender() {
        let game = Game::new(0, 10);
        assert_eq!(Game::attack_cost(2, 100, Stance::Explorer), Some((3, 7)));
        assert_eq!(Game::attack_cost(2, 100, Stance::Aggro), Some((4, 9)));
        assert_eq!(Game::attack_cost(6, 100, Stance::Explorer), None);
        let defended = game.defense_strength(game.players[1].base, 1);
        assert_eq!(defended, 2);
        let neutral = Game::index(0, 0, DEFAULT_BOARD_SIZE);
        assert_eq!(game.defense_strength(neutral, 1), 1);
    }

    #[test]
    fn four_seats_use_three_stances_and_rotate_matchups() {
        for simulation in 0..12 {
            let game = Game::new(simulation, 10);
            let counts = game
                .players
                .iter()
                .fold([0; STANCE_COUNT], |mut counts, player| {
                    counts[player.stance.index()] += 1;
                    counts
                });
            assert_eq!(counts.iter().sum::<usize>(), PLAYER_COUNT);
            assert_eq!(counts.iter().filter(|&&count| count == 2).count(), 1);
            assert_eq!(counts.iter().filter(|&&count| count == 1).count(), 2);
        }
    }

    #[test]
    fn batch_counts_duplicate_stance_as_one_match_winner() {
        let game = Game::new(0, 10);
        let wins = winning_stances(&game, &[5, 5, 1, 0]);
        assert_eq!(wins, BTreeSet::from([Stance::Explorer.index()]));
    }

    #[test]
    fn supplied_score_excludes_enclaves_and_counts_independent_outposts() {
        let map = MapDefinition::parse(&format!(
            "A.........B\n{}C.........D",
            "...........\n".repeat(9)
        ))
        .unwrap();
        let mut game = Game::from_map(0, 10, map);
        let bridge = Game::index(1, 0, 11);
        let connected = Game::index(2, 0, 11);
        let isolated = Game::index(3, 2, 11);
        for index in [bridge, connected, isolated] {
            game.cells[index].owner = Some(0);
        }
        game.refresh_networks();
        assert_eq!(game.total_fields(0), 4);
        assert_eq!(game.score(0), 3);
        game.cells[bridge].owner = None;
        game.refresh_networks();
        assert_eq!(game.total_fields(0), 3);
        assert_eq!(game.score(0), 1);
        game.cells[isolated].outpost = true;
        game.refresh_networks();
        assert_eq!(game.total_fields(0), 3);
        assert_eq!(game.score(0), 2);
        game.cells[bridge].owner = Some(0);
        game.refresh_networks();
        assert_eq!(game.total_fields(0), 4);
        assert_eq!(game.score(0), 4);
    }

    #[test]
    fn winners_use_supplied_fields_without_a_total_fields_tiebreaker() {
        let map = MapDefinition::parse(&format!(
            "A.........B\n{}C.........D",
            "...........\n".repeat(9)
        ))
        .unwrap();
        let mut game = Game::from_map(0, 10, map);
        for index in [
            Game::index(2, 2, 11),
            Game::index(3, 2, 11),
            Game::index(2, 3, 11),
        ] {
            game.cells[index].owner = Some(0);
        }
        game.cells[Game::index(4, 0, 11)].owner = Some(2);
        game.cells[Game::index(4, 0, 11)].outpost = true;
        game.refresh_networks();
        assert!(game.total_fields(0) > game.total_fields(2));
        let scores: Vec<_> = (0..PLAYER_COUNT).map(|player| game.score(player)).collect();
        assert_eq!(
            winning_stances(&game, &scores),
            BTreeSet::from([game.players[2].stance.index()])
        );
        game.cells[Game::index(1, 0, 11)].owner = Some(0);
        game.refresh_networks();
        let scores: Vec<_> = (0..PLAYER_COUNT).map(|player| game.score(player)).collect();
        assert_eq!(
            winning_stances(&game, &scores),
            BTreeSet::from([
                game.players[0].stance.index(),
                game.players[2].stance.index()
            ])
        );
    }

    #[test]
    fn ownership_change_keeps_network_stock_without_duplication() {
        let mut game = Game::new(0, 10);
        let home_network = game.cell_network[game.players[0].base].unwrap();
        game.networks.get_mut(&home_network).unwrap().stock = 19;
        let owned_tile = Game::index(2, 1, DEFAULT_BOARD_SIZE);
        game.cells[owned_tile].owner = Some(0);
        game.refresh_networks();
        let after = game.cell_network[game.players[0].base].unwrap();
        assert_eq!(game.networks[&after].stock, 19);
    }

    #[test]
    fn game_stops_at_configured_tick_limit() {
        let mut game = Game::new(0, 2);
        game.tick();
        game.tick();
        game.tick();
        assert_eq!(game.tick, 2);
        assert!(game.is_finished());
    }

    #[test]
    fn ap_purchases_use_escalating_prices_and_stop_at_six_ap() {
        let mut game = Game::new(0, 10);
        game.players[0].ap = 1;
        let home_network = game.cell_network[game.players[0].base].unwrap();
        for network in game.networks.values_mut() {
            network.stock = 0;
        }
        game.networks.get_mut(&home_network).unwrap().stock = 620;

        game.purchase_ap(0);

        assert_eq!(game.players[0].ap, MAX_AP);
        assert_eq!(game.networks[&home_network].stock, 0);
        assert_eq!(game.players[0].spent, 620);
        assert!(
            game.log
                .front()
                .unwrap()
                .contains("bought 5 AP for 620 supply")
        );
    }

    #[test]
    fn ap_purchase_cannot_combine_supply_from_separate_networks() {
        let mut game = Game::new(0, 10);
        game.players[0].ap = 1;
        let home_network = game.cell_network[game.players[0].base].unwrap();
        let outpost = Game::index(2, 2, DEFAULT_BOARD_SIZE);
        game.cells[outpost].owner = Some(0);
        game.cells[outpost].outpost = true;
        game.refresh_networks();
        let outpost_network = game.cell_network[outpost].unwrap();
        assert_ne!(home_network, outpost_network);

        game.networks.get_mut(&home_network).unwrap().stock = 19;
        game.networks.get_mut(&outpost_network).unwrap().stock = 1;
        game.purchase_ap(0);
        assert_eq!(game.players[0].ap, 1);
        assert_eq!(game.networks[&home_network].stock, 19);
        assert_eq!(game.networks[&outpost_network].stock, 1);

        game.networks.get_mut(&home_network).unwrap().stock = 20;
        game.purchase_ap(0);
        assert_eq!(game.players[0].ap, 2);
        assert_eq!(game.networks[&home_network].stock, 0);
        assert_eq!(game.networks[&outpost_network].stock, 1);
        assert_eq!(game.players[0].spent, 20);
    }

    #[test]
    fn visual_tick_frames_show_the_resolved_tick() {
        let mut game = Game::new(0, 2);
        let frames = game.tick_frames();
        assert_eq!(frames.len(), 1);
        assert!(
            frames
                .iter()
                .any(|frame| !frame.last_action_targets.is_empty())
        );
        assert_eq!(frames.last().unwrap().cells, game.cells);
        assert_eq!(game.tick, 1);
    }

    #[test]
    fn bots_never_choose_to_attack_a_home_base() {
        let mut game = Game::new(0, 10);
        game.players[0].stance = Stance::Aggro;
        let path: Vec<_> = (2..=8)
            .map(|x| Game::index(x, 1, DEFAULT_BOARD_SIZE))
            .collect();
        for cell in path {
            game.cells[cell].owner = Some(0);
        }
        game.refresh_networks();
        let network_id = game.cell_network[game.players[0].base].unwrap();
        game.networks.get_mut(&network_id).unwrap().stock = 100;
        let action = game
            .choose_action(0, &HashSet::new())
            .expect("bot has other legal actions");
        assert_ne!(action.target, game.players[1].base);
    }

    #[test]
    fn home_connected_outpost_does_not_accumulate_strain() {
        let mut game = Game::new(0, 10);
        let outpost = Game::index(2, 1, DEFAULT_BOARD_SIZE);
        game.cells[outpost].owner = Some(0);
        game.cells[outpost].outpost = true;
        game.refresh_networks();
        game.check_outposts();
        assert!(!game.outpost_strain.contains_key(&outpost));
        assert!(game.cells[outpost].outpost);
    }

    #[test]
    fn isolated_small_outpost_collapses_after_three_checks() {
        let mut game = Game::new(0, 10);
        let outpost = Game::index(7, 7, DEFAULT_BOARD_SIZE);
        game.cells[outpost].owner = Some(0);
        game.cells[outpost].outpost = true;
        game.refresh_networks();
        for _ in 0..3 {
            game.check_outposts();
        }
        assert!(!game.cells[outpost].outpost);
        assert_eq!(game.players[0].outposts_lost, 1);
    }

    #[test]
    fn image_map_parses_bases_and_whole_wall_tiles() {
        let options = Options {
            batch: None,
            ticks: DEFAULT_TICKS,
            frame_delay_ms: DEFAULT_FRAME_DELAY_MS,
            generated_size: None,
            map_path: None,
        };
        let map = load_selected_map(&options).unwrap();
        assert_eq!(map.width, 35);
        assert_eq!(map.height, 31);
        assert_eq!(map.name, "maps/crossroads.map");
        assert_eq!(
            map.bases,
            [
                Game::index(16, 14, 35),
                Game::index(18, 14, 35),
                Game::index(16, 16, 35),
                Game::index(18, 16, 35),
            ]
        );
        let wall = Game::index(17, 15, 35);
        assert!(map.cells[wall].wall);
        assert_eq!(map.cells.iter().filter(|cell| cell.wall).count(), 13);
        assert!(!map.cells[Game::index(17, 14, 35)].base);
        let expected_bases = map.bases;
        let game = Game::from_map(0, 10, map);
        assert_eq!(
            game.players
                .iter()
                .map(|player| player.base)
                .collect::<Vec<_>>(),
            expected_bases
        );
        assert!(game.players.iter().all(|player| {
            game.cells[player.base].owner == Some(player.name as usize - 'A' as usize)
                && game.cells[player.base].base
        }));
        assert!(game.cells[wall].wall);
    }

    #[test]
    fn distance_and_supply_use_the_base_position_from_the_map() {
        let mut game = Game::from_map(
            0,
            10,
            load_selected_map(&Options {
                batch: None,
                ticks: DEFAULT_TICKS,
                frame_delay_ms: DEFAULT_FRAME_DELAY_MS,
                generated_size: None,
                map_path: None,
            })
            .unwrap(),
        );
        let base = Game::index(16, 14, 35);
        let adjacent = Game::index(16, 13, 35);
        assert_eq!(game.players[0].base, base);
        assert_eq!(game.base_distances[0][base], 0);
        assert_eq!(game.base_distances[0][adjacent], 1);
        assert!(game.base_distances[0][Game::index(1, 1, 35)] > 1);

        game.cells[adjacent].owner = Some(0);
        game.refresh_networks();
        let network = game.cell_network[base].unwrap();
        let stock_before = game.networks[&network].stock;
        game.produce();
        assert_eq!(game.players[0].produced, 3);
        assert_eq!(game.networks[&network].stock, stock_before + 3);
    }

    #[test]
    fn text_maps_reject_invalid_rows_and_missing_bases() {
        let invalid_row = format!("{}\n", ".".repeat(15));
        assert!(MapDefinition::parse(&invalid_row).is_err());

        let missing_base = DEFAULT_MAP.replace('D', ".");
        assert!(MapDefinition::parse(&missing_base).is_err());
    }

    #[test]
    fn configurable_board_sizes_have_correct_dimensions_and_walls() {
        for size in [11, 17, 25] {
            let game = Game::with_board_size(0, 10, size);
            assert_eq!(game.cells.len(), size * size);
            assert_eq!(game.score(0), 1);
            let center = size / 2;
            let wall = Game::index(center, center - 3, size);
            assert!(game.cells[wall].wall);
            assert!(
                !game
                    .neighbors(Game::index(center - 1, center - 3, size))
                    .contains(&wall)
            );
        }
    }

    #[test]
    fn board_size_validation_requires_odd_supported_dimensions() {
        assert!(is_valid_board_size(11));
        assert!(is_valid_board_size(17));
        assert!(is_valid_board_size(25));
        assert!(!is_valid_board_size(10));
        assert!(!is_valid_board_size(12));
        assert!(!is_valid_board_size(103));
    }
}
