use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::dsp::eq::EQ_GAIN_RANGE_DB;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

pub use on_air_api::{EqResponse, SetEqRequest};

pub async fn get_eq(Paired: Paired, State(state): State<CoreState>) -> Json<EqResponse> {
    Json(EqResponse {
        gains_db: *state.eq_gains_db.lock(),
    })
}

pub async fn set_eq(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<SetEqRequest>,
) -> Result<StatusCode, ApiError> {
    // JSON cannot spell NaN, but `1e39` deserialises to +inf for f32 and a
    // non-finite gain would turn the biquads into silence generators.
    if req.gains_db.iter().any(|gain| !gain.is_finite()) {
        return Err(ApiError::validation("eq gains must be finite numbers"));
    }
    let clamped = req
        .gains_db
        .map(|g| g.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1));
    *state.eq_gains_db.lock() = clamped;
    state.remember_eq(clamped);
    Ok(StatusCode::NO_CONTENT)
}
