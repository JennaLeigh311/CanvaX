// Shared domain data models for canvas state, pixel updates, sessions, and users.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: Uuid,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pixel {
    pub canvas_id: Uuid,
    pub x: u32,
    pub y: u32,
    pub color: String,
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub canvas_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
}