# operator-surface Specification

## Purpose
TBD - created by archiving change operator-surface-v1. Update Purpose after archive.
## Requirements
### Requirement: Provide an optional HTTP metrics endpoint
synrepo MAY expose context metrics over HTTP when built with the `metrics-http` cargo feature. The endpoint SHALL NOT ship in the default build and SHALL default to a localhost bind with no authentication.

#### Scenario: Start the metrics server with the feature enabled
- **WHEN** an operator runs `synrepo server --metrics 127.0.0.1:9090` on a binary built with `--features metrics-http`
- **THEN** the process binds the supplied address and serves `GET /metrics` with the Prometheus exposition text produced by the shared metrics formatter
- **AND** the response body parses as valid Prometheus text

#### Scenario: Feature absent in the default build
- **WHEN** an operator runs `synrepo server --metrics 127.0.0.1:9090` on a binary built without `--features metrics-http`
- **THEN** the command exits non-zero with a message naming the required feature flag
- **AND** no port is bound

#### Scenario: Shared formatter with stdout export
- **WHEN** an operator compares `synrepo stats context --format prometheus` and the body of `GET /metrics`
- **THEN** both outputs are produced by the same formatter
- **AND** the metric names, counter types, and ordering match

