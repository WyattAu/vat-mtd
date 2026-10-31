# vat-mtd

HMRC Making Tax Digital (VAT); the typed submission contract, mapped from
[`vat-return`](https://crates.io/crates/vat-return)'s boxes.

## What this crate is, and what it deliberately is not

It is the **contract**: the exact JSON body a VAT return submission carries,
the arithmetic HMRC enforces between its fields, the fraud-prevention headers
every MTD call must carry, and the error envelope the API answers with.

It is **not an HTTP client**. Whether the caller uses reqwest, hyper, or a
queue-and-retry worker is the application's decision; coupling a tax contract
to a framework choice would make the contract harder to test and the framework
harder to change. Every type serialises with `serde`, so a transport is a few
lines at the call site and the contract's tests are fixture-exact.

## The money rule

HMRC's VAT-amount fields carry **two decimal places**; the value fields
(boxes 6 and 7) are **whole pounds** by filing law. Minor units convert to
decimal strings exactly; integer division and a zero-padded remainder, never
a float; and a value field carrying pence is **refused**, because silently
truncating 50p is a misstatement on a filing.

## The fraud-prevention headers

Every MTD call carries `Gov-Client-*` / `Gov-Vendor-*` headers whose required
set depends on the declared connection method. HMRC validates the *shape* of
each value and rejects the call with an error that does not name the offending
header; so the shapes (UUID device IDs, `vendor=x.y.z` versions) are checked
here, where the error can name them.

## Deliberate semantics

- `digitalSubmission` is always `true` from this crate: the figures came from
  digital records with digital links, and claiming otherwise is not a
  configuration.
- EU dispatches/acquisitions (the triangular-supply siblings of boxes 6 and 7)
  are explicit parameters; `vat-return` computes a UK-scope return, and
  inventing zeros here would under-declare for businesses that have them.
- A non-GBP return is the wrong return, not a conversion problem.

## Licence

MIT OR Apache-2.0.
