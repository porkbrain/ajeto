-- SQLite migrations

-- stores chat history
CREATE TABLE prompts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- the role is e.g "assistant" or "user"
    role TEXT NOT NULL,
    -- the prompt text
    content TEXT NOT NULL,
    -- approximation for how many tokens does the content is
    tokens INTEGER NOT NULL,
    -- the prompt's importance
    --
    -- search for karma docs to understand this
    karma INTEGER NOT NULL,
    -- under which mode did the system operate when this prompt was created
    -- see mode.rs for options
    --
    -- snake_case
    mode TEXT NOT NULL,

    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- stores misc settings as a map
CREATE TABLE settings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    value TEXT NOT NULL
);
-- defaults
INSERT INTO settings (name, value) VALUES ('openai_model', 'gpt-3.5-turbo');
INSERT INTO settings (name, value) VALUES ('openai_chat_endpoint', 'https://api.openai.com/v1/chat/completions');
INSERT INTO settings (name, value) VALUES ('recent_history_limit', '50');
INSERT INTO settings (name, value) VALUES ('max_thought_loops', '6');
INSERT INTO settings (name, value) VALUES ('system', 'Ubuntu 22.04');
INSERT INTO settings (name, value) VALUES ('default_karma', '1000');
INSERT INTO settings (name, value) VALUES ('instructive_karma', '10000');
INSERT INTO settings (name, value) VALUES ('contextual_karma', '2500');
INSERT INTO settings (name, value) VALUES ('decorative_karma', '500');
INSERT INTO settings (name, value) VALUES ('default_prompt', 'dali');
INSERT INTO settings (name, value) VALUES ('expiring_id_discount', '3.0');
INSERT INTO settings (name, value) VALUES ('expiring_created_at_discount', '300.0');
INSERT INTO settings (name, value) VALUES ('len_discount', '0.01');
INSERT INTO settings (name, value) VALUES ('karma_multiplier', '10.0');
INSERT INTO settings (name, value) VALUES ('max_tokens', '3500');
INSERT INTO settings (name, value) VALUES ('min_score_to_include_prompt', '0.00001');
INSERT INTO settings (name, value) VALUES ('max_bash_output_len_for_response', '2000');
INSERT INTO settings (name, value) VALUES ('end_session_after_idle_secs', '300');
