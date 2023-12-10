use std::collections::HashMap;

use crate::{
    actors::{
        redis_actor::SetRoomCommand, room_actor::RoomCreated, ClientWsActor, CreateRoom,
        GetRoomFieldCommand, JoinRoom, ListRooms, UpdateRoomFieldCommand,
    },
    models::messages::ServerCommand,
    AppState,
};
use actix_web::{http::StatusCode, HttpRequest, Query, State};
use futures::Future;

#[derive(Debug, Deserialize)]
pub struct QueryString {
    room_token: String,
    key: String,
    name: String,
}

pub fn socket_handler(
    (req, state, query): (HttpRequest<AppState>, State<AppState>, Query<QueryString>),
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    if crate::APP_CONFIG.dev_mode || crate::APP_CONFIG.api_keys.contains(&query.key) {
        let r = state
            .room_actor_addr
            .send(JoinRoom {
                room_token: query.room_token.clone(),
                api_key: query.key.clone(),
                team_name: query.name.clone(),
            })
            .wait()
            .unwrap();
        match r {
            Ok(room) => {
                let mut player_count: u32 = 0;
                if let Ok(player_count_str) = state.redis_actor_addr.send(GetRoomFieldCommand {
                    room_token: query.room_token.clone(),
                    field: String::from("in_room_players"),
                }).wait()? {
                    player_count = player_count_str
                        .parse::<u32>()
                        .map_err(|err| actix_web::error::ErrorInternalServerError(err.to_string()))?;
                }
                player_count = player_count + 1;
                let _ = state.redis_actor_addr.send(UpdateRoomFieldCommand {
                    room_token: query.room_token.clone(),
                    field: String::from("in_room_players"),
                    value: format!("{}", player_count),
                });
                
                actix_web::ws::start(
                    &req,
                    ClientWsActor::new(room.game_addr, query.key.clone(), query.name.clone()),
                )
            },
            Err(err) => {
                Err(actix_web::error::ErrorBadRequest(err.to_string()))
            },
        }
    } else {
        Err(actix_web::error::ErrorBadRequest("Invalid API Key"))
    }
}

#[derive(Debug, Deserialize)]
pub struct InspectatorString {
    room_token: String,
}

pub fn spectate_handler(
    (req, state, query): (HttpRequest<AppState>, State<AppState>, Query<InspectatorString>),
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    // TODO(bschwind) - Make a separate spectator actor
    let r = state
        .room_actor_addr
        .send(JoinRoom {
            room_token: query.room_token.clone(),
            api_key: "SPECTATOR".to_string(),
            team_name: "SPECTATOR".to_string(),
        })
        .wait()
        .unwrap();
    match r {
        Ok(room) => actix_web::ws::start(
            &req,
            ClientWsActor::new(room.game_addr, "SPECTATOR".to_string(), "SPECTATOR".to_string()),
        ),
        Err(err) => Err(actix_web::error::ErrorBadRequest(err.to_string())),
    }
}

pub fn reset_handler(
    (_req, state): (HttpRequest<AppState>, State<AppState>),
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    state.game_addr.do_send(ServerCommand::Reset);
    Ok(actix_web::HttpResponse::with_body(StatusCode::OK, "done"))
}

#[derive(Debug, Deserialize)]
pub struct RoomCreateRequest {
    pub name: String,
    pub max_players: u32,
    pub time_limit_seconds: u32,
}

pub fn create_room_handler(
    (_req, state, json): (
        HttpRequest<AppState>,
        State<AppState>,
        actix_web::Json<RoomCreateRequest>,
    ),
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let r = state
        .room_actor_addr
        .send(CreateRoom {
            name: json.name.clone(),
            max_players: json.max_players,
            time_limit_seconds: json.time_limit_seconds,
        })
        .wait();
    match r {
        Ok(room) => {
            let body = serde_json::to_string(&room).unwrap();

            // cache created room info
            let cache_fields = create_room_fields(&room);
            let _ = state
                .redis_actor_addr
                .send(SetRoomCommand { room_token: room.token, fields: cache_fields })
                .wait();
            Ok(actix_web::HttpResponse::with_body(StatusCode::OK, body))
        },
        Err(_) => Err(actix_web::error::ErrorBadRequest("Failed to create room")),
    }
}

pub fn list_rooms_handler(
    (_req, state): (HttpRequest<AppState>, State<AppState>),
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let room_map = state.room_actor_addr.send(ListRooms);
    match room_map.wait() {
        Ok(room_list) => {
            let body = serde_json::to_string(&room_list.rooms).unwrap();
            Ok(actix_web::HttpResponse::with_body(StatusCode::OK, body))
        },
        Err(_) => Err(actix_web::error::ErrorBadRequest("Failed to list rooms")),
    }
}

fn create_room_fields(room: &RoomCreated) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), room.name.clone());
    fields.insert("time_limit_seconds".to_string(), room.time_limit_seconds.to_string());
    fields.insert("in_room_players".to_string(), String::from("0"));
    fields.insert("status".to_string(), String::from("ready"));

    fields
}
