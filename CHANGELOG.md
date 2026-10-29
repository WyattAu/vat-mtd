# Changelog

All notable changes to this project are documented here. Format: [Keep a
Changelog](https://keepachangelog.com/) — versions follow [semver](https://semver.org).

## [Unreleased]

## [0.1.6] - 2026-10-29

### Changed

- Tracks `vat-return 0.1.10` (the invoice-kit 1.2.0 convergence). No behavioural change.


## [0.1.5] - 2026-10-29

### Changed

- Tracks `invoice-kit 1.2.0` (BR-AE-02 enforcement and the ReverseCharge rate fix). No behavioural change to this crate.


## [0.1.4] - 2026-10-27

### Changed

- Tracks `vat-return 0.1.8` (the converged pin set after the version race). No behavioural change.


## [0.1.3] - 2026-10-27

### Changed

- Tracks `vat-return 0.1.7` (the converged pin set). No behavioural change.


## [0.1.2] - 2026-10-27

### Changed

- Tracks `vat-return 0.1.6` (ap-kit 0.1.5 convergence). No behavioural change.


## [0.1.1] - 2026-10-27

### Changed

- Tracks `vat-return 0.1.5` — the estate graph's vat-rules convergence pin chain. No behavioural change.


## [0.1.0] - 2026-10-25

Initial release.

### Added

- `build_submission` — a `vat-return`'s boxes plus EU goods figures to the
  exact MTD submission body, with every derived total computed and the value
  fields refused if they carry pence.
- `FraudPrevention` — the `Gov-Client-*` / `Gov-Vendor-*` header set per
  connection method, with value shapes checked where the error can name the
  header.
- `parse_obligations`, `parse_receipt`, `HmrcApiError::parse` — the response
  contracts, fixture-tested against the documented envelopes.
