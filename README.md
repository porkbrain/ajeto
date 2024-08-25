# LLM deamon.

It listens to commands over http.
A UTF-8 string in the body of the request is the input on `POST /v1`.
It connects to OpenAI APIs for chatbot completions.
It has different modes of operation (e.g. bash, coding, ...)

## Modes

The input to the model determines its output.
Hence, controlling the history and context is the .
Modes provide capabilities and context to user input.
_What I say -> what I want to happen._

### `marat`

_Will you execute them for me, s'il vous plait?_

> Q: leading figures of the french revolution
> A: ... Maximilien Robespierre ... Georges Danton ... Jean-Paul Marat ... King Louis XVI

For when I want it to `$ bash`, I use _"marat"_ as the keyword that changes the mode.
A keyword which changes the mode is called a directive.
A good directive is

- short because you say it often;
- unambiguously pronounced with accents;
- mnemonic for flatter learning curve;
- obscure as in infrequently used.

What do I want from this mode?

**I** Show me current dir and list files
**O** It runs `pwd` and `ls` and shows me the output.

**I** Go up one dir level. Next change to the `example` dir. Next make sure that we're on the develop branch. Next log the past 10 commits into stdout.
**O** Prints the last 10 commit messages.

**I** Git add `example.rs`. Next commit with a relevant message.
**O** Uses git with a good summary.

### `dali`

For when I want a free-style mode, I use directive _"dali"_.
This mode is the most creative.
The prompt is the chat history and the instruction, without any system decorators.

**I** Summarize the commit messages in two sentences.

**O** Human friendly summary.

### `cobol`

_not implemented nor designed yet_

For when I want it to code, I use directive _"cobol"_.

**I** Focus on lines N to I
**O** ?

**I** Rewrite function F such that X
**O** ?

**I** Compile
**O** ?

**I** Compile and fix
**O** ?

- TODO: lean heavily into `git` and diffing

## Karma

The input to an LLM is limited.
Karma points are awarded to prompts to heuristically rank their value.
A decision on whether some history should be included is made based on a sorted list.

The framework upon which karma works categorizes prompts in this order of importance:

- **operative**; user explicitly promoted it
- **instructive**; the system considers it crucial information
- **default**; average prompt (also undetermined)
- **decorative**; correction, help or format specifiers, which are not relevant beyond a few interactions

Karma points affect prompt score positively, ie. increase its chance to be included in the next LLM input.

The age of a prompt in minutes affects the score negatively.

The relative distance, in number of prompts between the considered and the latest one, affects the score negatively.

The length of a prompt (in tokens) affects the score negatively.

We use following settings to scale the effects of each respectively mentioned parameter:

- `ξ` as `karma_multiplier` (eg. 10)
- `τ` as `expiring_created_at_discount` (eg. 300.0)
- `ζ` as `expiring_id_discount` (eg. 3.0)
- `φ` as `len_discount` (eg. 0.01)

The score equation where
`n` is the latest prompt,
`|i|` is the length of prompt `i`,
`k_i` is the karma of prompt `i`,
`t_i` are the minutes elapsed since creation of prompt `i`:

```
s_i =
    (ξ k_i)
    min( 1 , (1 / ( φ |i| )) )
    min( 1 , (1 / ( τ t_i )) )
    min( 1 , (1 / ( ζ (n - i) )) )
```

## Configuration

We use `settings` table to configure some behavior of the system.
The plan is to make the `settings` editable with a special mode.

- [**`openai_model`**][openai-api-chat]
- [**`openai_chat_endpoint`**][openai-api-chat]
- **`system`** dictates for what system should it write bash commands
- **`default_prompt`** dictates which prompt to use upon starting a new session
- **`end_session_after_idle_secs`** dictates after how many seconds we use `default_prompt` for next user input
- **`max_thought_loops`** dictates how many times at most can the system interact with LLM automatically in a loop
- **`max_tokens`** dictates how many tokens can be sent to the LLM at most
- **`max_bash_output_len_for_response`** dictates after how many chars do we truncate the output of running a bash command
- **`recent_history_limit`** dictates how many prompts from DB to select for consideration (they get pruned based on [karma](#karma))
- [**`default_karma`**](#karma)
- [**`instructive_karma`**](#karma)
- [**`contextual_karma`**](#karma)
- [**`decorative_karma`**](#karma)
- [**`expiring_id_discount`**](#karma)
- [**`expiring_created_at_discount`**](#karma)
- [**`len_discount`**](#karma)
- [**`karma_multiplier`**](#karma)
- [**`min_score_to_include_prompt`**](#karma)

## Install

You'll need sqlite:

```bash
apt-get install libsqlite3-dev
```

The docker version has only been tested on Ubuntu.
The image installs necessary dependencies.
Then use `$ ./tests/start-docker.sh` to compile binaries, create directories and run `docker-compose`.

At the moment, the container runs as root.
This means the created assets in `.tmp` directory have root as the owner.

See [`.env.example`](.env.example) for the _minimal_ environment variables setup.
See [`src/conf.rs`](src/conf.rs) for full list of envs.
Name ENVs after the properties of `Conf` struct, uppercase.

<!-- List of References -->

[openai-api-chat]: https://platform.openai.com/docs/api-reference/chat
