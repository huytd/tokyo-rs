use std::{
    collections::HashMap,
    io::{Error, ErrorKind, Result},
    sync::{
        atomic::{AtomicU32, Ordering},
        RwLock,
    },
};

use actix::{Actor, Addr, Context, Handler, Message, MessageResult};
use rand::{distributions::Alphanumeric, Rng};
use tokyo::models::GameConfig;

use super::GameActor;

// Actor to manage multiple rooms
pub struct RoomActor {
    config: GameConfig,
    id_counter: AtomicU32, // Counters, flags, shared state, fine-grained synchronization.
    rooms: RwLock<HashMap<String, Room>>, // RwLock: A reader-writer lock - Allow at most one writer at any point in time
}

// Represents a single room
struct Room {
    id: u32,
    name: String,
    max_players: u32,
    time_limit_seconds: u32,
    token: String,
    game: Addr<GameActor>,
}

#[derive(Message)]
#[rtype(result = "RoomCreated")]
pub struct CreateRoom {
    pub name: String,
    pub max_players: u32,
    pub time_limit_seconds: u32,
}

#[derive(Message, Deserialize, Serialize)]
pub struct RoomCreated {
    pub id: String,
    pub name: String,
    pub max_players: u32,
    pub time_limit_seconds: u32,
    pub token: String,
}

#[derive(Message)]
#[rtype(result = "Result<RoomJoined>")]
pub struct JoinRoom {
    pub room_token: String,
    pub api_key: String,
    pub team_name: String,
}

#[derive(Message)]
pub struct RoomJoined {
    pub game_addr: Addr<GameActor>,
    pub room_token: String,
    pub api_key: String,
    pub team_name: String,
}

#[derive(Message)]
#[rtype(result = "RoomList")]
pub struct ListRooms;

#[derive(Message, Serialize, Deserialize)]
pub struct RoomList {
    pub rooms: Vec<RoomDetail>,
}

#[derive(Message, Serialize, Deserialize)]
pub struct RoomDetail {
    pub id: u32,
    pub name: String,
    pub max_players: u32,
    pub time_limit_seconds: u32,
    pub token: String,
}

impl Room {
    pub fn new(
        config: &GameConfig,
        id: u32,
        name: String,
        max_players: u32,
        time_limit_seconds: u32,
        token: String,
    ) -> Room {
        let game_actor = GameActor::new(config.clone());
        let game_actor_addr = game_actor.start();

        Room { id, name, max_players, time_limit_seconds, token, game: game_actor_addr }
    }
}

impl RoomActor {
    pub fn new(cfg: GameConfig) -> RoomActor {
        RoomActor { config: cfg, id_counter: AtomicU32::new(0), rooms: RwLock::new(HashMap::new()) }
    }

    pub fn create_room(
        &mut self,
        name: String,
        max_players: u32,
        time_limit_seconds: u32,
    ) -> RoomCreated {
        let current_id_counter = self.id_counter.fetch_add(1, Ordering::SeqCst);
        let new_id = current_id_counter.to_string();
        let token: String =
            rand::thread_rng().sample_iter(&Alphanumeric).take(7).map(char::from).collect();

        self.rooms.get_mut().unwrap().insert(
            token.clone(),
            Room::new(
                &self.config,
                current_id_counter,
                name.to_string(),
                max_players,
                time_limit_seconds,
                token.clone(),
            ),
        );

        RoomCreated { id: new_id, name, max_players, time_limit_seconds, token }
    }
}

impl Actor for RoomActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("RoomActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("RoomActor started");
    }
}

impl Handler<CreateRoom> for RoomActor {
    type Result = MessageResult<CreateRoom>;

    fn handle(&mut self, msg: CreateRoom, _ctx: &mut Self::Context) -> Self::Result {
        let room = self.create_room(msg.name, msg.max_players, msg.time_limit_seconds);
        MessageResult(room)
    }
}

impl Handler<JoinRoom> for RoomActor {
    type Result = MessageResult<JoinRoom>;

    fn handle(&mut self, msg: JoinRoom, _ctx: &mut Self::Context) -> Self::Result {
        let room_map = self.rooms.read().unwrap();
        let room = room_map.get(&msg.room_token);
        match room {
            Some(room) => {
                let msg = RoomJoined {
                    game_addr: room.game.clone(),
                    room_token: msg.room_token,
                    api_key: msg.api_key,
                    team_name: msg.team_name,
                };
                MessageResult(Result::Ok(msg))
            },
            None => MessageResult(Result::Err(Error::new(ErrorKind::NotFound, "Room not found"))),
        }
    }
}

impl Handler<ListRooms> for RoomActor {
    type Result = MessageResult<ListRooms>;

    fn handle(&mut self, _msg: ListRooms, _ctx: &mut Self::Context) -> Self::Result {
        let room_map = self.rooms.read().unwrap();
        let rooms: Vec<&Room> = room_map.values().collect();
        let mut rooms: Vec<RoomDetail> = rooms
            .iter()
            .map(|room| RoomDetail {
                id: room.id,
                name: room.name.clone(),
                max_players: room.max_players,
                time_limit_seconds: room.time_limit_seconds,
                token: room.token.clone(),
            })
            .collect();
        rooms.sort_by_key(|room| room.id);
        MessageResult(RoomList { rooms })
    }
}
