# Tauri Backend Architecture

## Layering

- `commands/*`: Tauri command adapters. They only translate transport data and map service errors to string responses.
- `services/*`: application orchestration layer. Handles use-case flow and typed `AppError` boundaries.
- `config/*`: config schema, migration and validation. `ConfigStore` reads and writes through this boundary.
- `domain/*`: core entities and enums.
- `proxy/*`, `quota/*`, `remote_sync/*`: infrastructure/runtime modules.

## Dependency Rules

- `commands` can depend on `services`, `app_state`, `models`.
- `commands` must not depend directly on `proxy`, `quota`, `remote_sync`.
- `services` must not depend on `commands`.
- `config` must not depend on `commands` or `services`.

These rules are enforced by `scripts/check-tauri-boundaries.sh` and CI.

## Error Flow

- Service layer returns `AppResult<T>` (`services/error.rs`).
- `AppError` variants: `Validation`, `NotFound`, `External`, `Internal`.
- Command layer converts `AppError` to `String` for Tauri invoke response compatibility.

## Config Flow

1. Raw JSON is loaded by `ConfigStore`.
2. `config::migrator::migrate_config` upgrades old payloads to `CURRENT_CONFIG_VERSION`.
3. `config::schema::normalize_config` fills defaults and normalizes shape.
4. `config::validator::validate_config` validates constraints.
5. Normalized config is persisted, and in-memory state is refreshed.

## Test Strategy

- Unit tests stay close to modules (`proxy`, `mappers`, `quota/parser`, `remote_sync`).
- Contract tests use fixtures under `src-tauri/src/contract_fixtures`.
- CI runs targeted `contract_` tests in addition to full Rust tests.
