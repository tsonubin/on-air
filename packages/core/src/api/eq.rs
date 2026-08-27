use crate::auth::Paired;
use crate::dsp::eq::EQ_GAIN_RANGE_DB;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct EqResponse {
    pub gains_db: [f32; 5],
}

pub async fn get_eq(Paired: Paired, State(state): State<CoreState>) -> Json<EqResponse> {
    Json(EqResponse {
        gains_db: *state.eq_gains_db.lock().unwrap(),
    })
}

#[derive(Deserialize)]
pub struct SetEqRequest {
    pub gains_db: [f32; 5],
}

pub async fn set_eq(Paired: Paired, State(state): State<CoreState>, Json(req): Json<SetEqRequest>) -> StatusCode {
    let clamped = req
        .gains_db
        .map(|g| g.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1));
    *state.eq_gains_db.lock().unwrap() = clamped;
    StatusCode::NO_CONTENT
}
