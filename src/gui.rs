use std::{path::PathBuf, time::Duration};

use bevy::{
    asset::AssetId,
    input::mouse::{MouseButton, MouseWheel},
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

use crate::{
    Cell, Game, MAX_AP, MapDefinition, PLAYER_COUNT, RECENT_ACTIVITY_LIMIT, STANCE_NAMES,
    UNSUPPLIED_TICK_LIMIT, create_log_directory,
};

const SIDEBAR_PERCENT: f32 = 32.0;
const INITIAL_WINDOW_WIDTH: f32 = 1440.0;
const INITIAL_WINDOW_HEIGHT: f32 = 900.0;
const TILE_GAP: f32 = 1.5;
const INITIAL_LOG_ALPHA: f32 = 0.8;
const LOG_ALPHA_STEP: f32 = 0.1;
const UNSUPPLIED_BRIGHTNESS: f32 = 0.4;
const UI_FONT_DATA: &[u8] = include_bytes!("..\\assets\\fonts\\NotoSansJP.ttf");
const PALETTE: [Color; PLAYER_COUNT] = [
    Color::srgb_u8(45, 145, 210),
    Color::srgb_u8(207, 165, 34),
    Color::srgb_u8(47, 157, 91),
    Color::srgb_u8(151, 63, 165),
];

#[derive(Resource)]
struct GuiState {
    game: Game,
    map: MapDefinition,
    ticks: u32,
    frame_delay: Duration,
    running: bool,
    elapsed: Duration,
    selected_cell: Option<usize>,
    log_directory: PathBuf,
    error: Option<String>,
    log_alpha: f32,
}

#[derive(Resource, Default)]
struct PointerState {
    press_position: Option<Vec2>,
    last_position: Option<Vec2>,
    dragged: bool,
}

#[derive(Resource)]
struct BoardLayout {
    tile_size: f32,
}

#[derive(Resource, Default)]
struct MapViewportOffset(f32);

#[derive(Component)]
struct MapCamera;

#[derive(Component)]
struct CellTile(usize);

#[derive(Component)]
struct BaseLabel;

#[derive(Component)]
struct SelectionOutline;

#[derive(Component)]
struct StructureMarker {
    cell: usize,
    outpost: bool,
}

#[derive(Component)]
struct SupplyCountdown(usize);

#[derive(Component)]
struct TickText;

#[derive(Component)]
struct DetailText;

#[derive(Component)]
struct PlayerText(usize);

#[derive(Component)]
struct ActivityBackground;

#[derive(Component)]
struct OverlayActivityText;

pub fn run_gui(
    ticks: u32,
    frame_delay: Duration,
    map: MapDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    let font = Font::try_from_bytes(UI_FONT_DATA.to_vec())?;
    let log_directory = create_log_directory()?;
    let game = Game::from_map(0, ticks, map.clone());
    let tile_size = fit_tile_size(map.width, map.height);
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb_u8(14, 16, 19)))
        .insert_resource(GuiState {
            game,
            map,
            ticks,
            frame_delay,
            running: true,
            elapsed: Duration::ZERO,
            selected_cell: None,
            log_directory,
            error: None,
            log_alpha: INITIAL_LOG_ALPHA,
        })
        .insert_resource(PointerState::default())
        .insert_resource(BoardLayout { tile_size })
        .insert_resource(MapViewportOffset::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Borderlands".to_string(),
                resolution: (INITIAL_WINDOW_WIDTH as u32, INITIAL_WINDOW_HEIGHT as u32).into(),
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_keyboard,
                handle_map_input,
                advance_game,
                update_camera_viewport,
                update_board
                    .after(handle_keyboard)
                    .after(handle_map_input)
                    .after(advance_game),
                update_sidebar,
            ),
        );
    app.world_mut()
        .resource_mut::<Assets<Font>>()
        .insert(AssetId::default(), font)?;
    app.run();
    let state = app.world().resource::<GuiState>();
    state.game.write_logs(&state.log_directory)?;
    println!("Game logs saved in {}", state.log_directory.display());
    Ok(())
}

fn fit_tile_size(width: usize, height: usize) -> f32 {
    let map_width = INITIAL_WINDOW_WIDTH * (1.0 - SIDEBAR_PERCENT / 100.0);
    let map_height = INITIAL_WINDOW_HEIGHT;
    ((map_width - 80.0) / width as f32)
        .min((map_height - 80.0) / height as f32)
        .max(5.0)
}

fn setup(mut commands: Commands, state: Res<GuiState>, board_layout: Res<BoardLayout>) {
    commands.spawn((Camera2d, MapCamera));

    let half_width = (state.game.board_width as f32 - 1.0) / 2.0;
    let half_height = (state.game.board_height as f32 - 1.0) / 2.0;
    for (index, cell) in state.game.cells.iter().enumerate() {
        let x = index % state.game.board_width;
        let y = index / state.game.board_width;
        let center = Vec3::new(
            (x as f32 - half_width) * board_layout.tile_size,
            (half_height - y as f32) * board_layout.tile_size,
            0.0,
        );
        commands.spawn((
            Sprite::from_color(
                tile_color(&state.game, index),
                Vec2::splat(board_layout.tile_size - TILE_GAP),
            ),
            Transform::from_translation(center),
            CellTile(index),
        ));
        if !cell.wall {
            spawn_structure_markers(&mut commands, index, center, board_layout.tile_size);
            commands.spawn((
                Text2d::new(""),
                TextFont {
                    font_size: board_layout.tile_size * 0.45,
                    ..default()
                },
                TextColor(Color::srgb_u8(145, 148, 153)),
                Transform::from_translation(center + Vec3::Z * 3.5),
                Visibility::Hidden,
                SupplyCountdown(index),
            ));
        }
        if cell.base {
            commands.spawn((
                Text2d::new(state.game.players[cell.owner.unwrap()].name.to_string()),
                TextFont {
                    font_size: board_layout.tile_size * 0.5,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_translation(Vec3::new(center.x, center.y, 2.0)),
                BaseLabel,
            ));
        }
    }

    spawn_selection_outline(&mut commands, board_layout.tile_size);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(SIDEBAR_PERCENT),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(18.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb_u8(24, 27, 32)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("BORDERLANDS"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb_u8(185, 193, 205)),
                TickText,
            ));
            panel.spawn((
                Text::new("FELTDETALJER\nKlik på et felt for at se detaljer."),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                DetailText,
            ));
            for player in 0..PLAYER_COUNT {
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(PALETTE[player]),
                    PlayerText(player),
                ));
            }
            panel.spawn((
                Text::new(
                    "Scroll: zoom   Drag: pan\nSpace: step   R: pause   N: new matchup\nF3: fade log   F4: show log   F12: fullscreen   Q: quit",
                ),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb_u8(135, 145, 158)),
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(12.0),
                width: Val::Percent(31.0),
                height: Val::Percent(42.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(12.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(activity_background_color(INITIAL_LOG_ALPHA)),
            ActivityBackground,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("RECENT ACTIVITY"),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb_u8(220, 224, 230)),
                OverlayActivityText,
            ));
        });
}

fn spawn_structure_markers(commands: &mut Commands, cell: usize, center: Vec3, tile_size: f32) {
    let usable_size = tile_size - TILE_GAP;
    for outpost in [true, false] {
        let parts = if outpost {
            vec![
                (Vec2::new(0.0, 0.08), Vec2::new(0.24, 0.36)),
                (Vec2::new(0.0, 0.28), Vec2::new(0.40, 0.10)),
                (Vec2::new(-0.15, 0.35), Vec2::new(0.10, 0.10)),
                (Vec2::new(0.15, 0.35), Vec2::new(0.10, 0.10)),
            ]
        } else {
            vec![
                (Vec2::new(0.0, -0.30), Vec2::new(0.60, 0.12)),
                (Vec2::new(-0.24, -0.20), Vec2::new(0.12, 0.12)),
                (Vec2::new(0.0, -0.20), Vec2::new(0.12, 0.12)),
                (Vec2::new(0.24, -0.20), Vec2::new(0.12, 0.12)),
            ]
        };
        commands
            .spawn((
                Transform::from_translation(center + Vec3::Z * 3.0),
                Visibility::Hidden,
                StructureMarker { cell, outpost },
            ))
            .with_children(|parent| {
                for (position, size) in parts {
                    parent.spawn((
                        Sprite::from_color(Color::srgb_u8(15, 18, 22), size * usable_size),
                        Transform::from_translation((position * usable_size).extend(0.0)),
                    ));
                    parent.spawn((
                        Sprite::from_color(
                            Color::srgb_u8(245, 241, 218),
                            size * usable_size * 0.75,
                        ),
                        Transform::from_translation((position * usable_size).extend(0.1)),
                    ));
                }
            });
    }
}

fn spawn_selection_outline(commands: &mut Commands, tile_size: f32) {
    let side = tile_size - TILE_GAP;
    let thickness = (side * 0.06).min(TILE_GAP);
    let offset = (side + thickness) / 2.0;
    commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, 4.0),
            Visibility::Hidden,
            SelectionOutline,
        ))
        .with_children(|parent| {
            for (position, size) in [
                (
                    Vec2::new(0.0, offset),
                    Vec2::new(side + thickness * 2.0, thickness),
                ),
                (
                    Vec2::new(0.0, -offset),
                    Vec2::new(side + thickness * 2.0, thickness),
                ),
                (Vec2::new(-offset, 0.0), Vec2::new(thickness, side)),
                (Vec2::new(offset, 0.0), Vec2::new(thickness, side)),
            ] {
                parent.spawn((
                    Sprite::from_color(Color::srgb_u8(255, 219, 92), size),
                    Transform::from_translation(position.extend(0.0)),
                ));
            }
        });
}

fn update_camera_viewport(
    window_query: Query<&Window, With<PrimaryWindow>>,
    mut camera_query: Query<(&mut Camera, &mut Transform), With<MapCamera>>,
    mut viewport_offset: ResMut<MapViewportOffset>,
) {
    let Ok(window) = window_query.single() else {
        return;
    };
    let Ok((mut camera, mut transform)) = camera_query.single_mut() else {
        return;
    };
    camera.viewport = None;
    let offset = window.width() * SIDEBAR_PERCENT / 200.0;
    transform.translation.x += offset - viewport_offset.0;
    viewport_offset.0 = offset;
}

fn tile_color(game: &Game, index: usize) -> Color {
    let cell = &game.cells[index];
    if cell.wall {
        Color::srgb_u8(59, 65, 75)
    } else if let Some(owner) = cell.owner {
        if game.cell_network[index].is_some() {
            PALETTE[owner]
        } else {
            let color = PALETTE[owner].to_srgba();
            Color::srgb(
                color.red * UNSUPPLIED_BRIGHTNESS,
                color.green * UNSUPPLIED_BRIGHTNESS,
                color.blue * UNSUPPLIED_BRIGHTNESS,
            )
        }
    } else {
        Color::srgb_u8(32, 33, 35)
    }
}

fn update_board(
    state: Res<GuiState>,
    board_layout: Res<BoardLayout>,
    mut tiles: Query<(&CellTile, &mut Sprite)>,
    mut outline: Query<
        (&mut Transform, &mut Visibility),
        (
            With<SelectionOutline>,
            Without<StructureMarker>,
            Without<SupplyCountdown>,
        ),
    >,
    mut markers: Query<
        (&StructureMarker, &mut Visibility),
        (Without<SelectionOutline>, Without<SupplyCountdown>),
    >,
    mut countdowns: Query<
        (&SupplyCountdown, &mut Text2d, &mut Visibility),
        (Without<SelectionOutline>, Without<StructureMarker>),
    >,
) {
    if state.is_changed() {
        for (tile, mut sprite) in &mut tiles {
            sprite.color = tile_color(&state.game, tile.0);
        }
        for (marker, mut visibility) in &mut markers {
            let cell = &state.game.cells[marker.cell];
            let built = if marker.outpost {
                cell.outpost
            } else {
                cell.fortification > 0
            };
            *visibility = if built && !cell.wall && cell.owner.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }

    for (countdown, mut text, mut visibility) in &mut countdowns {
        let cell = &state.game.cells[countdown.0];
        if cell.owner.is_some() && !cell.wall && state.game.cell_network[countdown.0].is_none() {
            text.0 = UNSUPPLIED_TICK_LIMIT
                .saturating_sub(cell.unsupplied_ticks)
                .to_string();
            *visibility = Visibility::Visible;
        } else {
            text.0.clear();
            *visibility = Visibility::Hidden;
        }
    }

    let Ok((mut transform, mut visibility)) = outline.single_mut() else {
        return;
    };
    if let Some(index) = state.selected_cell {
        let x = index % state.game.board_width;
        let y = index / state.game.board_width;
        transform.translation.x =
            (x as f32 - (state.game.board_width as f32 - 1.0) / 2.0) * board_layout.tile_size;
        transform.translation.y =
            ((state.game.board_height as f32 - 1.0) / 2.0 - y as f32) * board_layout.tile_size;
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn handle_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<GuiState>,
    mut exit: MessageWriter<AppExit>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        if !state.game.is_finished() {
            state.running = false;
            state.elapsed = Duration::ZERO;
            state.game.tick();
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        state.running = !state.running;
    }
    if keys.just_pressed(KeyCode::F3) {
        state.log_alpha = decrease_log_alpha(state.log_alpha);
    }
    if keys.just_pressed(KeyCode::F4) {
        state.log_alpha = (state.log_alpha + LOG_ALPHA_STEP).min(1.0);
    }
    if keys.just_pressed(KeyCode::F12)
        && let Ok(mut window) = window_query.single_mut()
    {
        window.mode = if matches!(&window.mode, WindowMode::Windowed) {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
    }
    if keys.just_pressed(KeyCode::KeyN) {
        save_game(&mut state);
        let simulation_no = state.game.simulation_no + 1;
        state.game = Game::from_map(simulation_no, state.ticks, state.map.clone());
        state.selected_cell = None;
        state.elapsed = Duration::ZERO;
        state.running = true;
    }
    if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Escape) {
        save_game(&mut state);
        exit.write(AppExit::Success);
    }
}

fn save_game(state: &mut GuiState) {
    match state.game.write_logs(&state.log_directory) {
        Ok(()) => state.error = None,
        Err(error) => {
            let message = format!("Could not save game logs: {error}");
            eprintln!("{message}");
            state.error = Some(message);
        }
    }
}

fn advance_game(time: Res<Time>, mut state: ResMut<GuiState>) {
    if !state.running || state.game.is_finished() {
        return;
    }
    state.elapsed += time.delta();
    if state.elapsed >= state.frame_delay {
        state.game.tick();
        state.elapsed = Duration::ZERO;
    }
}

fn handle_map_input(
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mut camera_query: Query<
        (&Camera, &GlobalTransform, &mut Transform, &mut Projection),
        With<MapCamera>,
    >,
    mut pointer: ResMut<PointerState>,
    mut state: ResMut<GuiState>,
) {
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        pointer.press_position = None;
        pointer.last_position = None;
        return;
    };
    let map_width = window.width() * (1.0 - SIDEBAR_PERCENT / 100.0);
    if cursor.x >= map_width {
        if buttons.just_released(MouseButton::Left) {
            pointer.press_position = None;
            pointer.dragged = false;
        }
        pointer.last_position = Some(cursor);
        return;
    }
    let Ok((camera, global_transform, mut transform, mut projection)) = camera_query.single_mut()
    else {
        return;
    };

    for event in wheel.read() {
        if let Projection::Orthographic(projection) = &mut *projection {
            projection.scale = (projection.scale * (1.0 - event.y * 0.1)).clamp(0.25, 4.0);
        }
    }

    if buttons.just_pressed(MouseButton::Left) {
        pointer.press_position = Some(cursor);
        pointer.last_position = Some(cursor);
        pointer.dragged = false;
    }
    if buttons.pressed(MouseButton::Left)
        && let (Some(start), Some(last)) = (pointer.press_position, pointer.last_position)
    {
        pointer.dragged |= cursor.distance(start) > 4.0;
        if pointer.dragged {
            let delta = cursor - last;
            let scale = match &*projection {
                Projection::Orthographic(projection) => projection.scale,
                _ => 1.0,
            };
            transform.translation.x -= delta.x * scale;
            transform.translation.y += delta.y * scale;
        }
    }
    if buttons.just_released(MouseButton::Left) {
        if !pointer.dragged
            && let Ok(world_position) = camera.viewport_to_world_2d(global_transform, cursor)
        {
            state.selected_cell = world_to_cell(
                world_position,
                state.game.board_width,
                state.game.board_height,
                fit_tile_size(state.game.board_width, state.game.board_height),
            );
        }
        pointer.press_position = None;
        pointer.dragged = false;
    }
    pointer.last_position = Some(cursor);
}

fn world_to_cell(position: Vec2, width: usize, height: usize, tile_size: f32) -> Option<usize> {
    let x = (position.x / tile_size + width as f32 / 2.0).floor() as isize;
    let y = (height as f32 / 2.0 - position.y / tile_size).floor() as isize;
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        None
    } else {
        Some(y as usize * width + x as usize)
    }
}

fn update_sidebar(
    state: Res<GuiState>,
    mut tick_text: Query<&mut Text, With<TickText>>,
    mut detail_text: Query<&mut Text, (With<DetailText>, Without<TickText>)>,
    mut player_text: Query<(&PlayerText, &mut Text), (Without<TickText>, Without<DetailText>)>,
    mut activity_text: Query<
        &mut Text,
        (
            With<OverlayActivityText>,
            Without<TickText>,
            Without<DetailText>,
            Without<PlayerText>,
        ),
    >,
    mut activity_background: Query<&mut BackgroundColor, With<ActivityBackground>>,
) {
    if let Ok(mut text) = tick_text.single_mut() {
        let status = if state.game.is_finished() {
            "FINISHED"
        } else if state.running {
            "RUNNING"
        } else {
            "PAUSED"
        };
        text.0 = format!(
            "{}  |  Tick {}/{}  |  {}",
            state.game.map_name, state.game.tick, state.game.max_ticks, status
        );
    }

    if let Ok(mut text) = detail_text.single_mut() {
        text.0 = field_details(&state.game, state.selected_cell);
    }

    for (player_text, mut text) in &mut player_text {
        let player = &state.game.players[player_text.0];
        let score = state.game.score(player_text.0);
        let total = state.game.total_fields(player_text.0);
        let stock: u32 = state
            .game
            .networks
            .values()
            .filter(|network| network.owner == player_text.0)
            .map(|network| network.stock)
            .sum();
        let outposts = state
            .game
            .cells
            .iter()
            .filter(|cell| cell.owner == Some(player_text.0) && cell.outpost)
            .count();
        text.0 = format!(
            "{}  {}\n   Forsynede {}  |  Uforsynede {}  |  I alt {}\n   AP {}/{}  |  supply {}  |  outposts {}\n   Combat +{}  |  Discovery +{}  |  attacks {}",
            player.name,
            STANCE_NAMES[player.stance.index()],
            score,
            total - score,
            total,
            player.ap,
            MAX_AP,
            stock,
            outposts,
            player.fields_won_in_combat,
            player.fields_taken_by_discovery,
            player.attacks,
        );
    }

    if let Ok(mut text) = activity_text.single_mut() {
        let mut content = recent_activity_text(&state.game);
        if let Some(error) = &state.error {
            content.push_str(&format!("\n{error}"));
        }
        text.0 = content;
    }
    if let Ok(mut background) = activity_background.single_mut() {
        background.0 = activity_background_color(state.log_alpha);
    }
}

fn decrease_log_alpha(alpha: f32) -> f32 {
    if alpha <= LOG_ALPHA_STEP {
        0.0
    } else {
        alpha - LOG_ALPHA_STEP
    }
}

fn activity_background_color(alpha: f32) -> Color {
    Color::srgba_u8(14, 16, 19, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn recent_activity_text(game: &Game) -> String {
    game.log
        .iter()
        .take(RECENT_ACTIVITY_LIMIT)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn field_details(game: &Game, selected: Option<usize>) -> String {
    let Some(index) = selected else {
        return "FELTDETALJER\nKlik på et felt for at se detaljer.".to_string();
    };
    let cell: &Cell = &game.cells[index];
    let (x, y) = Game::xy(index, game.board_width);
    let mut details = format!("FELT ({}, {})", x + 1, y + 1);
    if cell.wall {
        details.push_str("\nEjer: —\nVærdi: —\nVæg");
        return details;
    }
    if let Some(owner) = cell.owner {
        let value = if game.cell_network[index].is_none() {
            0
        } else if cell.base {
            2
        } else {
            1 + (game.base_distances[owner][index] / 4) as u32
        };
        let defense = game.defense_strength(index, owner);
        let supply = if game.cell_network[index].is_some() {
            "Forsynet"
        } else {
            "Uforsynet"
        };
        details.push_str(&format!(
            "\nEjer: {} ({})\nVærdi: {} forsyning/tick\nForsvar: {}\n{}",
            game.players[owner].name,
            STANCE_NAMES[game.players[owner].stance.index()],
            value,
            defense,
            supply,
        ));
        if game.cell_network[index].is_none() {
            details.push_str(&format!(
                "\nNeutral om {} ticks",
                UNSUPPLIED_TICK_LIMIT.saturating_sub(cell.unsupplied_ticks)
            ));
        }
    } else {
        details.push_str("\nEjer: Neutral\nVærdi: 0 forsyning/tick");
    }
    details.push_str(&format!("\nStatus: {}", building_status(cell)));
    details
}

fn building_status(cell: &Cell) -> String {
    let mut structures = Vec::new();
    if cell.base {
        structures.push("Base");
    }
    if cell.outpost {
        structures.push("Outpost");
    }
    if cell.fortification > 0 {
        structures.push("Befæstning");
    }
    if structures.is_empty() {
        structures.push("Ubebygget");
    }
    format!("[{}]", structures.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_BOARD_SIZE;

    fn test_state() -> GuiState {
        let map = MapDefinition::generated(DEFAULT_BOARD_SIZE);
        GuiState {
            game: Game::from_map(0, 10, map.clone()),
            map,
            ticks: 10,
            frame_delay: Duration::from_millis(120),
            running: true,
            elapsed: Duration::ZERO,
            selected_cell: None,
            log_directory: PathBuf::new(),
            error: None,
            log_alpha: INITIAL_LOG_ALPHA,
        }
    }

    #[test]
    fn world_positions_map_to_board_cells() {
        assert_eq!(world_to_cell(Vec2::ZERO, 5, 5, 10.0), Some(12));
        assert_eq!(world_to_cell(Vec2::new(-20.0, 20.0), 5, 5, 10.0), Some(0));
        assert_eq!(world_to_cell(Vec2::new(30.0, 0.0), 5, 5, 10.0), None);
    }

    #[test]
    fn supply_countdown_shows_remaining_ticks_and_hides_when_supplied_or_neutral() {
        let mut state = test_state();
        let index = state
            .game
            .cells
            .iter()
            .position(|cell| !cell.wall && cell.owner.is_none())
            .unwrap();
        state.game.cells[index].owner = Some(0);
        let mut app = App::new();
        app.insert_resource(state)
            .insert_resource(BoardLayout { tile_size: 10.0 })
            .add_systems(Update, update_board);
        let entity = app
            .world_mut()
            .spawn((SupplyCountdown(index), Text2d::new(""), Visibility::Hidden))
            .id();
        for ticks in 0..5 {
            app.world_mut().resource_mut::<GuiState>().game.cells[index].unsupplied_ticks = ticks;
            app.update();
            assert_eq!(
                app.world().get::<Text2d>(entity).unwrap().0,
                (5 - ticks).to_string()
            );
            assert_eq!(
                *app.world().get::<Visibility>(entity).unwrap(),
                Visibility::Visible
            );
        }
        {
            let mut state = app.world_mut().resource_mut::<GuiState>();
            state.game.cell_network[index] = state.game.cell_network[state.game.players[0].base];
        }
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        {
            let mut state = app.world_mut().resource_mut::<GuiState>();
            state.game.cells[index].owner = None;
            state.game.cell_network[index] = None;
        }
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().get::<Text2d>(entity).unwrap().0.is_empty());
    }

    #[test]
    fn field_details_show_owner_value_and_structures() {
        let game = Game::new(0, 10);
        let details = field_details(&game, Some(game.players[0].base));
        assert!(details.contains("Ejer: A"));
        assert!(details.contains("Værdi: 2 forsyning/tick"));
        assert!(details.contains("Status: [Base]"));
        assert!(!details.contains(": Nej"));
    }

    #[test]
    fn building_status_covers_unbuilt_and_combined_structures() {
        let mut cell = Cell::neutral();
        assert_eq!(building_status(&cell), "[Ubebygget]");
        cell.outpost = true;
        assert_eq!(building_status(&cell), "[Outpost]");
        cell.fortification = 1;
        assert_eq!(building_status(&cell), "[Outpost | Befæstning]");
        cell.outpost = false;
        assert_eq!(building_status(&cell), "[Befæstning]");
        cell.base = true;
        assert_eq!(building_status(&cell), "[Base | Befæstning]");
    }

    #[test]
    fn neutral_field_details_show_unbuilt_status() {
        let game = Game::new(0, 10);
        let index = game
            .cells
            .iter()
            .position(|cell| !cell.wall && cell.owner.is_none())
            .unwrap();
        let details = field_details(&game, Some(index));
        assert!(details.contains("Ejer: Neutral"));
        assert!(details.contains("Status: [Ubebygget]"));
        assert!(!details.contains(": Nej"));
    }

    #[test]
    fn unsupplied_tiles_are_dimmer_without_changing_other_tile_colors() {
        let mut game = Game::new(0, 10);
        let index = game
            .cells
            .iter()
            .position(|cell| !cell.wall && cell.owner.is_none())
            .unwrap();
        let neutral_color = tile_color(&game, index);
        assert_eq!(neutral_color, Color::srgb_u8(32, 33, 35));
        let wall = game.cells.iter().position(|cell| cell.wall).unwrap();
        assert_eq!(tile_color(&game, wall), Color::srgb_u8(59, 65, 75));
        for owner in 0..PLAYER_COUNT {
            game.cells[index].owner = Some(owner);
            game.cell_network[index] = game.cell_network[game.players[owner].base];
            assert_eq!(tile_color(&game, index), PALETTE[owner]);
            game.cell_network[index] = None;
            let dimmed = tile_color(&game, index).to_srgba();
            let supplied = PALETTE[owner].to_srgba();
            assert_eq!(dimmed.red, supplied.red * UNSUPPLIED_BRIGHTNESS);
            assert_eq!(dimmed.green, supplied.green * UNSUPPLIED_BRIGHTNESS);
            assert_eq!(dimmed.blue, supplied.blue * UNSUPPLIED_BRIGHTNESS);
            assert_eq!(dimmed.alpha, 1.0);
        }
    }

    #[test]
    fn board_colors_follow_supply_changes_even_without_a_new_tick() {
        let mut app = App::new();
        let mut state = test_state();
        let index = state
            .game
            .cells
            .iter()
            .position(|cell| !cell.wall && cell.owner.is_none())
            .unwrap();
        state.game.cells[index].owner = Some(0);
        state.game.cell_network[index] = None;
        app.insert_resource(state)
            .insert_resource(BoardLayout { tile_size: 10.0 })
            .add_systems(Update, update_board);
        let entity = app
            .world_mut()
            .spawn((CellTile(index), Sprite::from_color(Color::WHITE, Vec2::ONE)))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().color,
            tile_color(&app.world().resource::<GuiState>().game, index)
        );
        assert_ne!(app.world().get::<Sprite>(entity).unwrap().color, PALETTE[0]);
        {
            let mut state = app.world_mut().resource_mut::<GuiState>();
            state.game.cell_network[index] = state.game.cell_network[state.game.players[0].base];
        }
        app.update();
        assert_eq!(app.world().resource::<GuiState>().game.tick, 0);
        assert_eq!(app.world().get::<Sprite>(entity).unwrap().color, PALETTE[0]);
        app.world_mut().resource_mut::<GuiState>().game.cell_network[index] = None;
        app.update();
        assert_ne!(app.world().get::<Sprite>(entity).unwrap().color, PALETTE[0]);
    }

    #[test]
    fn structure_markers_follow_building_and_removal() {
        let state = test_state();
        let index = state.game.players[0].base;
        let mut app = App::new();
        app.insert_resource(state)
            .insert_resource(BoardLayout { tile_size: 10.0 })
            .add_systems(Startup, move |mut commands: Commands| {
                spawn_structure_markers(&mut commands, index, Vec3::ZERO, 10.0);
                spawn_selection_outline(&mut commands, 10.0);
            })
            .add_systems(Update, update_board);
        for (outpost, fortification) in [(false, 0), (true, 0), (true, 1), (false, 1), (false, 0)] {
            {
                let mut state = app.world_mut().resource_mut::<GuiState>();
                state.game.cells[index].outpost = outpost;
                state.game.cells[index].fortification = fortification;
            }
            app.update();
            let world = app.world_mut();
            let mut query = world.query::<(&StructureMarker, &Visibility, &Children)>();
            let mut count = 0;
            for (marker, visibility, children) in query.iter(world) {
                count += 1;
                assert_eq!(children.len(), 8);
                let expected = if marker.outpost {
                    outpost
                } else {
                    fortification > 0
                };
                assert_eq!(
                    *visibility,
                    if expected {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    }
                );
            }
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn selection_is_an_empty_frame_and_preserves_the_tile_color() {
        let mut state = test_state();
        let index = state.game.players[0].base;
        state.selected_cell = Some(index);
        let color = tile_color(&state.game, index);
        let tile_size = 10.0;
        let mut app = App::new();
        app.insert_resource(state)
            .insert_resource(BoardLayout { tile_size })
            .add_systems(Startup, move |mut commands: Commands| {
                spawn_selection_outline(&mut commands, tile_size);
            })
            .add_systems(Update, update_board);
        let tile = app
            .world_mut()
            .spawn((
                CellTile(index),
                Sprite::from_color(color, Vec2::splat(tile_size - TILE_GAP)),
            ))
            .id();
        app.update();
        assert_eq!(app.world().get::<Sprite>(tile).unwrap().color, color);
        let world = app.world_mut();
        let mut query = world
            .query_filtered::<(Entity, &Transform, &Visibility, &Children), With<SelectionOutline>>(
            );
        let (entity, transform, visibility, children) = query.single(world).unwrap();
        let outline = entity;
        assert_eq!(*visibility, Visibility::Visible);
        let (x, y) = Game::xy(index, DEFAULT_BOARD_SIZE);
        assert_eq!(
            transform.translation.x,
            (x as f32 - (DEFAULT_BOARD_SIZE as f32 - 1.0) / 2.0) * tile_size
        );
        assert_eq!(
            transform.translation.y,
            ((DEFAULT_BOARD_SIZE as f32 - 1.0) / 2.0 - y as f32) * tile_size
        );
        assert!(world.get::<Sprite>(outline).is_none());
        assert_eq!(children.len(), 4);
        for child in children {
            let sprite = world.get::<Sprite>(*child).unwrap();
            let position = world.get::<Transform>(*child).unwrap().translation;
            let size = sprite.custom_size.unwrap();
            let half_side = (tile_size - TILE_GAP) / 2.0;
            assert!(
                position.x.abs() - size.x / 2.0 >= half_side - 0.0001
                    || position.y.abs() - size.y / 2.0 >= half_side - 0.0001
            );
            assert_eq!(sprite.color.to_srgba().alpha, 1.0);
        }
        app.world_mut().resource_mut::<GuiState>().selected_cell = None;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(outline).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(app.world().get::<Sprite>(tile).unwrap().color, color);
    }

    #[test]
    fn bundled_font_has_danish_and_hiragana_glyphs() {
        let font = Font::try_from_bytes(UI_FONT_DATA.to_vec()).unwrap();
        let mut system = bevy::text::CosmicFontSystem::default();
        system.0.db_mut().load_font_data(font.data.as_ref().clone());
        let id = system.0.db().faces().next().unwrap().id;
        let loaded = system.0.get_font(id).unwrap();
        for character in "æøåÆØÅ"
            .chars()
            .chain(('ぁ'..='ゖ').chain(['゙', '゚', 'ゝ', 'ゞ', 'ゟ']))
        {
            assert!(
                loaded.rustybuzz().glyph_index(character).is_some(),
                "Missing glyph: {character}"
            );
        }
    }

    #[test]
    fn recent_activity_contains_only_the_latest_twenty_entries() {
        let mut game = Game::new(0, 10);
        for index in 0..25 {
            game.log(format!("activity-{index}"));
        }

        let lines: Vec<_> = recent_activity_text(&game)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(lines.len(), RECENT_ACTIVITY_LIMIT);
        assert!(lines[0].contains("activity-5"));
        assert!(lines[19].contains("activity-24"));
    }

    #[test]
    fn activity_log_alpha_can_be_reduced_to_zero() {
        let mut alpha = INITIAL_LOG_ALPHA;
        for _ in 0..8 {
            alpha = decrease_log_alpha(alpha);
        }
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn sidebar_queries_have_disjoint_text_access() {
        let mut app = App::new();
        app.insert_resource(test_state())
            .add_systems(Update, update_sidebar);
        app.update();
    }

    #[test]
    fn player_panel_shows_supplied_unsupplied_and_total_fields() {
        let mut state = test_state();
        let isolated = state
            .game
            .cells
            .iter()
            .position(|cell| !cell.wall && cell.owner.is_none())
            .unwrap();
        state.game.cells[isolated].owner = Some(0);
        state.game.cell_network[isolated] = None;
        let mut app = App::new();
        app.insert_resource(state)
            .add_systems(Update, update_sidebar);
        let entity = app.world_mut().spawn((PlayerText(0), Text::new(""))).id();
        app.update();
        let text = &app.world().get::<Text>(entity).unwrap().0;
        assert!(text.contains("Forsynede 1  |  Uforsynede 1  |  I alt 2"));
    }
}
