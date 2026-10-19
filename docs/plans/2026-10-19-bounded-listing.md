# Design — bound the device listing as one operation

2026-10-19 · branch `omp/improve-2026-10-19`

## Problem

`GET /api/outputs` lists Bluetooth devices through
`packages/core/src/sender/bluetooth_host.rs::list()`, which shells out to
`bluetoothctl` **once for the device list and then once per device** for
`bluetoothctl info <addr>`.

Each call is now bounded to 3 seconds (that was a separate fix on this branch —
before it, one wedged call hung the request forever). But the bound is *per
call*, so the total is serial and unbounded in the number of devices:

| paired devices | worst case for one `GET /api/outputs` |
| --- | --- |
| 1 | 6 s |
| 5 | 18 s |
| 15 | 48 s |

That is a long time for a request the phone makes on a timer while a user is
looking at a device list. And the failure mode is the worst kind: the host is
already misbehaving (no bluetoothd, wedged system bus), so every call burns its
full timeout before the next one starts.

`list()` is also called from `pulse_sink_address`'s callers during pairing, so
the cost is paid on more than one path.

## Why it is worth fixing

The per-call bound converted an *infinite* hang into a *long* one. That is a real
improvement but it leaves the request path slow precisely on the machines least
able to absorb it. `on-air` targets old hardware and a wedged bus is a
realistic state, not a hypothetical.

## Approach

Give the whole listing one **wall-clock budget** rather than N independent
budgets. The first call is the cheap one — it tells us how many devices there
are — so the interesting case is "the bus is already wedged", where we want to
find out fast and return rather than pay a timeout per device.

Concretely:

1. `bluetoothctl` takes a deadline rather than a fixed 3 s. A call that would
   overrun the deadline is skipped entirely (or run with the remaining time).
2. `list()` takes a single budget — the same order as today's single-call bound,
   not N times it. On expiry it returns whatever it has collected so far and
   logs once.
3. Returning partial results beats returning none: a half-filled device list is
   more useful to a user than an empty one, and the next poll refines it.

The budget is a constant next to the existing per-call one, so the two read as
one policy rather than two unrelated numbers.

## What I am deliberately not doing

- **Caching the device list.** Attractive but it changes staleness semantics and
  needs an invalidation story for pairing and hotplug. That is a bigger design
  and belongs with the discovery work, not here.
- **Parallelising the `info` calls.** Spawning N subprocesses on a wedged bus
  makes the problem worse and adds no value when the bus is the bottleneck.
- **Retrying.** A wedged bus does not recover because we asked twice.

## Failure behaviour

Budget exhausted → return the devices already described, plus any whose `info`
was never fetched (defaulting to the paired-but-unknown shape `list()` already
produces when `info` fails). Never panic, never block past the budget.

## Testing

The regression is "one listing call must return inside its budget even when
`bluetoothctl` never exits". Per the test-audit gates I will not add an
injection hook or mutate `PATH` for a test — that is exactly the seam the skill
rejects. The real boundary proof is `tests/serve_integration.rs`, which already
exercises `GET /api/outputs`; it passed once the per-call bound landed and must
keep passing. Where a tighter assertion is not reachable without a seam, I will
say so rather than invent one.
