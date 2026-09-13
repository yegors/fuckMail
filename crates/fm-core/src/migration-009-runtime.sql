-- Non-secret references and connection leases; tokens remain in the OS vault.
ALTER TABLE google_accounts ADD COLUMN connection_generation INTEGER NOT NULL DEFAULT 1;
CREATE TABLE google_account_clients (
    account TEXT PRIMARY KEY REFERENCES google_accounts(account),
    client_id TEXT NOT NULL,
    config_path TEXT NOT NULL
);
