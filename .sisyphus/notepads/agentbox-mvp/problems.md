# Problems

## 2026-04-21 Final Wave blocker

- `SPEC.md` is internally contradictory for workspace naming: it normatively says to derive `hash12` from SHA-256 of the canonical git-root path bytes, but its worked `/aaa/bbb` example requires `2f83c6a14d91`, which does not match the literal SHA-256 result for `/aaa/bbb`.
- Final-wave F4 cannot approve until one rule is declared authoritative: the literal algorithm or the worked example.
- Resolution: the literal SHA-256 algorithm is authoritative, so the previous `/aaa/bbb` special-case was removed and the worked example now follows the algorithmic output.
