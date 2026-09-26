# OAuth test key

The RSA DER private key in this directory was generated solely to sign synthetic JWTs in tests. It is intentionally public test material, has no Google or account access, and must never be used outside tests. `jwks.json` is its public key. Tests build fixed synthetic claims in memory; no real ID, access or refresh token is present.

The tests cover signatures, nonce/state/PKCE, subject/audience/issuer/time checks, callback handling, scope reporting, refresh coordination and keychain namespace isolation. They do not exercise live Google consent or refresh, or a locked OS keychain.
