/// Sample-rate catalogs and snapping for the input/output bridge.
///
/// Capture always lands on the user-selected *input* rate (pipeline rate).
/// Each transport then resamples that stream to its *output* rate, which must
/// be one of the rates that transport actually supports.

pub const STANDARD_RATES_HZ: &[u32] = &[16_000, 32_000, 44_100, 48_000, 88_200, 96_000];

/// Rates we offer for capture / pipeline processing when a device does not
/// advertise a tighter list.
pub const INPUT_RATES_HZ: &[u32] = &[44_100, 48_000, 88_200, 96_000];

/// Sonos HTTP PCM / WAV radio sources.
pub const SONOS_RATES_HZ: &[u32] = &[44_100, 48_000];

/// AirPlay 1 is 44.1 kHz; AirPlay 2 commonly also does 48 kHz.
pub const AIRPLAY_RATES_HZ: &[u32] = &[44_100, 48_000];

/// A2DP SBC sample rates.
pub const BLUETOOTH_RATES_HZ: &[u32] = &[16_000, 32_000, 44_100, 48_000];

pub fn transport_rates(transport: &str) -> &'static [u32] {
    match transport {
        "sonos" => SONOS_RATES_HZ,
        "airplay" => AIRPLAY_RATES_HZ,
        "bluetooth" => BLUETOOTH_RATES_HZ,
        _ => STANDARD_RATES_HZ,
    }
}

/// Keep catalog entries that a device claims to support (`min..=max`).
pub fn rates_in_range(min_hz: u32, max_hz: u32, catalog: &[u32]) -> Vec<u32> {
    catalog
        .iter()
        .copied()
        .filter(|rate| *rate >= min_hz && *rate <= max_hz)
        .collect()
}

pub fn is_supported(rate_hz: u32, supported: &[u32]) -> bool {
    supported.contains(&rate_hz)
}

/// Prefer `preferred` when listed; otherwise the nearest supported rate.
pub fn snap_rate(preferred_hz: u32, supported: &[u32]) -> u32 {
    if supported.is_empty() {
        return preferred_hz;
    }
    if supported.contains(&preferred_hz) {
        return preferred_hz;
    }
    *supported
        .iter()
        .min_by_key(|rate| rate.abs_diff(preferred_hz))
        .unwrap_or(&supported[0])
}

/// Default output rate for a transport from its supported list.
pub fn default_output_rate(transport: &str, supported: &[u32]) -> u32 {
    let prefer = match transport {
        "bluetooth" => 48_000,
        _ => 44_100,
    };
    snap_rate(prefer, supported)
}

/// Intersect a device-advertised range list with a transport catalog.
/// `device_ranges` is `(min_hz, max_hz)` per config; empty means "unknown,
/// use the catalog as-is".
pub fn intersect_catalog(device_ranges: &[(u32, u32)], catalog: &[u32]) -> Vec<u32> {
    if device_ranges.is_empty() {
        return catalog.to_vec();
    }
    let mut rates = Vec::new();
    for &(min_hz, max_hz) in device_ranges {
        for rate in rates_in_range(min_hz, max_hz, catalog) {
            if !rates.contains(&rate) {
                rates.push(rate);
            }
        }
    }
    rates.sort_unstable();
    if rates.is_empty() {
        catalog.to_vec()
    } else {
        rates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_picks_nearest_when_preferred_missing() {
        assert_eq!(snap_rate(44_100, &[48_000, 96_000]), 48_000);
        assert_eq!(snap_rate(48_000, &[44_100, 48_000]), 48_000);
        assert_eq!(snap_rate(32_000, &[16_000, 44_100]), 44_100);
    }

    #[test]
    fn bluetooth_defaults_to_48k_when_available() {
        assert_eq!(default_output_rate("bluetooth", BLUETOOTH_RATES_HZ), 48_000);
        assert_eq!(default_output_rate("sonos", SONOS_RATES_HZ), 44_100);
    }

    #[test]
    fn intersect_keeps_catalog_when_device_range_unknown() {
        assert_eq!(intersect_catalog(&[], SONOS_RATES_HZ), vec![44_100, 48_000]);
        assert_eq!(
            intersect_catalog(&[(48_000, 48_000)], BLUETOOTH_RATES_HZ),
            vec![48_000]
        );
    }
}
