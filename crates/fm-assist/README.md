# fm-assist

Optional local-model assistance for fuckMail. A model running on the user's own
machine, through Ollama or any OpenAI-compatible server on a loopback address,
reads one received email at a time and returns a one-sentence summary, whether
a person or an automated system sent it, and at most one of the account's own
labels.

This crate holds the model-facing logic only. It performs no I/O: `fm-engine`
sends the HTTP requests and `fm-core` stores the settings and results.

## Contents

- **`ollama`**: the model settings (`Settings`, with per-account `Account`
  options), the built-in prompts (`src/system-prompt.txt`,
  `src/user-prompt.txt`) and their `{{messages}}`/`{{labels}}` variables, the
  request bodies for Ollama's `/api/chat` (`ollama_request`) and
  OpenAI-compatible `/v1/chat/completions` (`openai_request`), the JSON schema
  the reply must match (`schema`), and reply validation (`parse_reply`).
  `sample` returns a built-in test email from `fixtures/ollama-mail.json` for
  the settings screen's Test button.
- **Crate root**: `Boundary`, a job table between stored mail and a `Runner`.
  It keeps each job to one account, leaves out unpublished drafts, caps the
  context, queue and result sizes, lets each owner (a GUI or CLI session)
  cancel its own jobs, and rejects a result when the policy or any source
  changed while the job ran. The only output it passes on is a subset of the
  configured category IDs and an optional summary.

## What it does not do

- It never acts on mail. The per-label `auto_read` and `auto_spam` options
  are user settings: when the model picks such a label, `fm-core` applies
  that setting. Nothing in this crate can send, delete, write or mark mail.
- It accepts only labels the account already has. System labels such as
  `INBOX`, `SPAM` or `TRASH` are refused as model destinations.
- Endpoints must be `http://` loopback addresses (`127.0.0.1`, `localhost` or
  `[::1]`) with an explicit port. Prompt text treats the email and labels as
  data, never as instructions.
