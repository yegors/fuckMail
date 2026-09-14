# google-p2-auth-v1

The RSA DER private key in this directory was newly generated solely to sign synthetic local JWT fixtures. It is intentionally public test material, has no Google/account access, and must never be used outside tests. `jwks.json` is its public key. Tests create fixed synthetic claims in memory; no real ID/access/refresh token is present.

The corpus verifies signatures, nonce/state/PKCE, subject/audience/issuer/time, callback handling, scope reporting, refresh coordination and native-vault namespace isolation. Live provider consent/refresh and physical locked-keyring behavior remain separate gates.
