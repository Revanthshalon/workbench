# Spec: data

Module `data` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`contexts`](SPEC-contexts.md) (approved: connection settings, SSH tunnels, the remote runner), [`audit`](SPEC-audit.md) (draft: every tunnel and remote command is recorded) and [`core`](SPEC-core.md) (approved). Used by `execution-flow` and `flow-memory`. `operational-support` only; `developer-support` never loads it.

## Decisions already made elsewhere

1. **PostgreSQL, Redis and Kafka, read-only, in the active context**, reached through the context's VM. Each connection chooses `access: tunnel` (a local port forward on `127.0.0.1`, native drivers here, datastore credentials from the context file) or `access: remote` (the VM's own `psql`, `redis-cli` or Kafka tools run as typed templates through the remote runner; the datastore password never leaves this machine) (`SPEC-contexts.md`).
2. PostgreSQL and Redis can have several **named connections** per context; Kafka is one cluster per context.
3. **Read-only enforcement lives in our own code** for the tunnel path, and every layer is tested (`SPEC-contexts.md`, Assumption 3). `context check` proves a connection works, not that its credentials are read-only.
4. **Kafka reads never join a consumer group or commit offsets**, so reading never changes the system's state (map, Boundaries).
5. Tools: `db_query`, `db_schema`, `redis_command`, `datastore_health` and the Kafka tools, which this spec defines: `kafka_topics`, `kafka_lag`, `kafka_messages`. All are kind `observe`; every answer names its context.
6. Secrets (datastore passwords) are `secrecy` values exposed only to open a connection; nothing is scrubbed by pattern, resolved secrets are removed from output by exact match (`SPEC-audit.md`).

## Assumptions (correct me now)

1. **One engine port per system, two adapters each.** `SqlClient`, `RedisClient` and `KafkaClient` are ports in the engine. Each has a **tunnel** adapter (a native driver over the tunnel) and a **remote** adapter (typed templates through the runner). Both must give the same observable answers, proved by one contract suite per port that runs against a fake and, with `--ignored`, against a real datastore in each mode.
2. **The guard comes first, on both paths.** Every query or command is checked by the engine's guard before any adapter sees it. A rejection names the rule. The adapters add their own layer after it (a read-only transaction, a bounded read), never instead of it.
3. **Data is sensitive.** Production rows can hold personal data. The defence in the first release is bounded answers (rows, cell size, total size, time), the guard, and the audit log recording what was asked and, up to 64 KB, what came back (`SPEC-audit.md`). Masking columns or denying tables is not in the first release (Open Question 1).
4. **Limits (defaults, and the most a call may raise them to):** SQL rows 100 (1 000); Redis elements per reply 500; Kafka messages 10 (200); a cell, value or message body 4 KB (8 KB for a Kafka message), cut with a marker and the full size; a whole answer 1 MB; a query's time 10 seconds (30). On the remote path the runner's own limits also apply (30 seconds, 5 MB).
5. **Big numbers stay exact.** `bigint`, `numeric` and `decimal` are returned as strings; `bytea` as `\x` hex (cut at the cell limit); `json` and `jsonb` as JSON; timestamps as ISO 8601 with the offset; everything else as the driver gives it.
6. **Remote PostgreSQL takes no bound parameters.** `psql` through a template cannot bind safely, so on `access: remote` a call with `params` is refused and the guard checks the literals in the text. On `tunnel`, `params` are bound server-side (`$1`, `$2`).
7. **Tunnels are local only.** A tunnel binds an ephemeral port on `127.0.0.1`, closes with the SSH connection or after 10 idle minutes, and is recorded as a `tunnel` event. Another local user on a shared machine could reach that port while it is open; the datastores' own credentials still apply, but a Redis with no password would be open to them (Open Question 6).
8. Client crates are new dependencies and need approval: a PostgreSQL driver with TLS (for example `tokio-postgres`), a Redis client, a Kafka client and a SQL parser (for example `sqlparser`). The Kafka client is chosen at build time (`rdkafka` needs `librdkafka` and supports SASL and SSL; a pure-Rust client may lack some SASL mechanisms).

## Objective

Let an agent ask the datastores of the active environment what is true (rows, keys, lag, health) without any way to change them, in answers small enough to read and safe enough to log.

The module is done when:
- each tool answers through either access mode with the same result, bounded and typed,
- every write attempt of every kind is rejected by the guard or, failing that, by the datastore's read-only layer, and the datastore is unchanged,
- Kafka reads leave no consumer group and no committed offset behind,
- `datastore_health` summarises each system with findings from fixed rules,
- everything asked and answered is visible in the session log.

## PostgreSQL

### `db_query`

Input: `connection` (optional when the context has one), `sql`, `params` (tunnel only), `limit`, `timeout_seconds`. Answer: `columns` (name, type), `rows`, `row_count`, `truncated`, `cut_cells`, `elapsed_ms`.

**The guard** parses `sql` with a real SQL parser (PostgreSQL dialect), never with patterns:

| Allowed | Everything else is refused |
|---|---|
| exactly one statement that is a `SELECT` (including set operations, and `WITH` whose CTEs are all queries), `EXPLAIN` of such a query without `ANALYZE`, or `SHOW` | `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `TRUNCATE`, DDL, `COPY`, `SET`, `RESET`, `DO`, `CALL`, `LISTEN`, `NOTIFY`, `VACUUM`, `SELECT … INTO`, locking clauses (`FOR UPDATE`, `FOR SHARE`, …), a data-modifying CTE, more than one statement, and any text the parser cannot parse |

Two more refusals inside an allowed statement:
- **Functions** with effects outside the query, by name (schema-qualified or not, case-insensitive): `set_config`, `pg_sleep*`, `pg_terminate_backend`, `pg_cancel_backend`, `pg_reload_conf`, `pg_rotate_logfile`, `pg_advisory_*`, `pg_notify`, `nextval`, `setval`, `lo_*`, `pg_read_file`, `pg_read_binary_file`, `pg_ls_dir`, `pg_stat_file`, `pg_stat_reset*`, `dblink*`.
- **Relations** that expose credentials: `pg_authid`, `pg_shadow`, `pg_user_mapping`, `pg_user_mappings`, `pg_subscription`, `pg_largeobject`.

**The layers after the guard:**
1. The row limit is applied by wrapping the validated `SELECT` in `SELECT * FROM (…) AS q LIMIT n+1`, so one more row than the limit shows that it was cut. `EXPLAIN` and `SHOW` are small and not wrapped.
2. The session sets `default_transaction_read_only = on`, `statement_timeout`, `lock_timeout` (2 seconds) and `idle_in_transaction_session_timeout` first. The statement runs in a `READ ONLY` transaction that is always rolled back. PostgreSQL enforces this itself, so it is a real second layer. The credentials should still be a read-only role; Workbench cannot verify that.
3. On `remote`, the template passes these settings as `psql` options (`-X`, `--no-psqlrc`, `-w`, `ON_ERROR_STOP`) and the statement as one argument; the runner's time and output limits apply.

### `db_schema`

Input: `connection`, `schema` (default: every non-system schema), `table` (contains), `limit`. Answer: schemas; per table the columns (name, type, nullable, default), primary key, foreign keys, indexes, approximate row count (`pg_class.reltuples`) and the comment. Built from fixed catalog queries, not from user text, and it never reads the denied relations.

## Redis

### `redis_command`

Input: `connection`, `command` (an array: the command and its arguments, each a string up to 1 KB). One command per call; the connection's `db` is used and `SELECT` is refused. The guard allows **only** this list, with each command's argument shape checked:

| Group | Allowed |
|---|---|
| Keys | `GET`, `MGET` (at most 100 keys), `EXISTS`, `TYPE`, `TTL`, `PTTL`, `STRLEN`, `RANDOMKEY`, `DBSIZE`, `SCAN` (`COUNT` at most 1 000), `OBJECT ENCODING`, `OBJECT IDLETIME`, `MEMORY USAGE` |
| Hash, set, sorted set | `HGET`, `HMGET`, `HLEN`, `HSCAN`, `SCARD`, `SISMEMBER`, `SSCAN`, `ZCARD`, `ZSCORE`, `ZRANK`, `ZSCAN` |
| Bounded ranges | `LRANGE`, `ZRANGE`, `XRANGE` — an explicit range of at most 500 elements, never `0 -1` on an unbounded collection |
| Whole collections | `HGETALL`, `HKEYS`, `HVALS`, `SMEMBERS` — only after the guard has checked `HLEN` or `SCARD` is at most 500, otherwise refused with a hint to use `HSCAN` or `SSCAN` |
| Lists and streams | `LLEN`, `LINDEX`, `XLEN`, `XINFO STREAM`, `XINFO GROUPS` |
| Server | `PING`, `TIME`, `INFO` (the sections `server`, `clients`, `memory`, `stats`, `replication`, `persistence`, `keyspace` only), `PUBSUB CHANNELS`, `PUBSUB NUMSUB` |

Everything else is refused by name, including `KEYS` (blocks a server), `CONFIG`, `CLIENT`, `SLOWLOG`, `MONITOR`, `DEBUG`, `EVAL`, `FUNCTION`, `SCRIPT`, `MODULE`, `PFCOUNT` and `GETEX` (they can change state or expose credentials), blocking commands, `SUBSCRIBE` and every write. Replies are typed (string, integer, list, map, null, error) and cut by the limits; on `remote` the reply is parsed from `redis-cli`'s text output, so types are as `redis-cli` prints them.

## Kafka

| Tool | Input | Answer |
|---|---|---|
| `kafka_topics` | `topic` (contains), `include_internal` (default false), `limit` | Topics with partition count, replication factor, per-partition earliest and latest offset and an estimated message count, and an allow-list of topic settings (`retention.ms`, `retention.bytes`, `cleanup.policy`, `min.insync.replicas`, `max.message.bytes`). Broker settings are never read |
| `kafka_lag` | `group` (optional), `topic` (optional) | Consumer groups with state and member count; per topic-partition the committed offset, the end offset and the lag; total lag per topic |
| `kafka_messages` | `topic`, `partition` (optional, default all), a start: `offset`, `timestamp`, or `last` (the last N), `limit`, `key` (exact), `contains` (text) | Messages with partition, offset, timestamp, key, headers and the value as text (UTF-8 or base64) with `is_json`; at most 5 000 messages are scanned for a filter, and the answer says when the scan stopped early |

**Reading changes nothing.** `kafka_lag` and `kafka_topics` use only metadata, offset and group-listing requests. `kafka_messages` assigns partitions directly and seeks, joins no group, and sets auto-commit off. On the tunnel path the reader has no committed offsets to write; on the remote path the console consumer is started with an explicit partition and offset and `enable.auto.commit=false`. **A test against a real broker must show that after a read no group exists that was not there before and no committed offset changed;** this is confirmed when the adapters are built and the answer is recorded in the spec's status. Avro, Protobuf and a schema registry are not decoded in the first release (Open Question 4).

## `datastore_health`

Input: `system` (`postgres`, `redis`, `kafka`, default all) and `connection`. Per system it returns reachability, latency, version and the figures below, then **findings** from fixed thresholds, each with the figure it came from.

| System | Figures | Findings |
|---|---|---|
| PostgreSQL | version, start time, connections used and `max_connections`, database size, age of the longest running query and of the oldest idle-in-transaction session (never the query text), count of sessions waiting on locks, replication lag when a replica, transaction-id age | `ConnectionsNearLimit` (over 80%), `LongRunningQuery` (over 5 minutes), `IdleInTransaction` (over 5 minutes), `Blocked` (any), `ReplicationLag` (over 30 seconds), `XidAgeHigh` (over 1 billion) |
| Redis | version, uptime, role, clients, used and max memory, evicted keys, hit ratio, last persistence status, replication lag | `MemoryNearLimit` (over 90% of a set `maxmemory`), `Evicting` (evictions rising since the last read), `PersistenceFailing`, `ReplicaDown` |
| Kafka | brokers, controller, topic and partition counts, partitions without a leader, partitions under-replicated | `OfflinePartitions`, `UnderReplicated`, `NoController` |

The thresholds are constants in the engine, named in the finding and covered by tests; a finding is a fact about a figure, not a diagnosis. The tool does not read query text, keys or message contents.

## Connections and failures

- A tool resolves the context's connection settings from the session's active context; secrets stay wrapped until the adapter opens the connection.
- If a context has no section for the system, the tool says it is not configured for this context. A name that is not configured lists the configured ones.
- A connection that cannot be opened fails the call naming the context, system, connection and the cause (never a secret); a datastore error carries its code (a PostgreSQL `SQLSTATE`, a Redis error prefix) with the message passed through the exact-match scrub.
- On `tunnel`, the SSH connection and the forward are opened on first use (the SSH side follows `SPEC-contexts.md`) and each opening is a `tunnel` audit event; on `remote`, each run is a `remote_command` event.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `serde_json`, `secrecy`, `thiserror`.
- New dependencies, each needing approval (Assumption 8): PostgreSQL driver with TLS, a Redis client, a Kafka client, a SQL parser.
- Tests against real datastores need local PostgreSQL, Redis and Kafka (Docker), only for `--ignored` runs and CI service containers.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Live:    cargo test --workspace -- --ignored    # against local PostgreSQL, Redis and Kafka, in tunnel and remote mode
```

## Project Structure

```
lib/engine/src/data/
├── ports.rs        # SqlClient, RedisClient, KafkaClient
├── limits.rs       # row, cell, size and time limits and the cut markers
├── sql_guard.rs    # parse, allow one SELECT/EXPLAIN/SHOW, deny functions and relations
├── redis_guard.rs  # the allow-list, argument shapes, range and cardinality checks
├── rows.rs         # typed result values (exact numbers, bytea, json, timestamps)
├── schema.rs       # the db_schema catalog queries and their result model
├── kafka.rs        # topics, lag and message reading plans, filters, decoding
└── health.rs       # figures, thresholds, findings

lib/adapters/src/data/
├── postgres/       # tunnel.rs (driver, read-only transaction), remote.rs (psql template)
├── redis/          # tunnel.rs (client), remote.rs (redis-cli template and parsing)
├── kafka/          # tunnel.rs (client, assign and seek), remote.rs (kafka tools templates and parsing)
└── suite.rs        # the contract suite per port, run against the fake and against each adapter

services/cli/src/mcp/modules/data.rs   # db_query, db_schema, redis_command, kafka_topics, kafka_lag, kafka_messages, datastore_health
```

The engine has no I/O: guards, limits, planning, parsing and findings are plain logic over the three ports and a fake.

## Code Style

The guard returns the reason, so the agent and the log both say why:

```rust
/// Why a statement or command was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("only a single SELECT, EXPLAIN or SHOW is allowed; found {0}")]
    Statement(String),
    #[error("function '{0}' is not allowed")]
    Function(String),
    #[error("relation '{0}' is not allowed")]
    Relation(String),
    #[error("Redis command '{0}' is not allowed")]
    Command(String),
}
```

Follow existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **SQL guard** (engine, no database): every refused form listed above is refused, including a write hidden in a CTE, a second statement after `;`, `SELECT … INTO`, `FOR UPDATE`, each denied function (also schema-qualified and in mixed case), each denied relation, and text the parser cannot parse; plain reads, joins, CTEs, set operations, `EXPLAIN` and `SHOW` pass.
- **Redis guard:** the allow-list is exact (a test lists it and fails on additions); `KEYS`, `CONFIG`, `CLIENT`, `SLOWLOG`, `EVAL`, writes and blocking commands are refused; unbounded ranges, oversized `COUNT` and `MGET`, `SELECT`, and `HGETALL` on a hash over 500 fields are refused or redirected with the hint.
- **Contract suite** per port, against the fake and, with `--ignored`, against a real PostgreSQL, Redis and Kafka **in tunnel and in remote mode**, with identical observable results: typed values (a `bigint` above 2^53, `numeric`, `bytea`, `jsonb`, timestamps), the row limit and `truncated`, cell cutting, time limit, and an error with its code.
- **Read-only proof (PostgreSQL, real database):** every refused statement, sent around the guard in a test harness, is rejected by the read-only transaction and the data is unchanged; `nextval` is refused.
- **Kafka non-interference (real broker, both modes):** after `kafka_messages`, `kafka_lag` and `kafka_topics`, the list of consumer groups and every committed offset are unchanged.
- **Limits:** each default and maximum applies; a limit above the maximum is lowered, not an error; cut markers carry the full size.
- **Health:** each finding triggers at its threshold and not below, from fixture figures; no query text, key or message content appears in any health answer.
- **Secrets:** the datastore password and a planted fake secret in an error message never appear in an answer or the log.
- **Audit:** every tunnel opening and remote command is an event; the query text is in the `tool_call` line.
- **Failures:** not configured; an unknown connection name lists the known ones; an unreachable datastore names context, system and connection.
- Every tool in the table and every group in the Redis allow-list has a test.

## Boundaries

- **Always:** run the guard before any adapter, on both paths; use a read-only transaction and timeouts for SQL; bound every answer and say when it was cut; keep Kafka reads out of consumer groups; name the context in every answer.
- **Ask first:** adding a Redis command or a SQL function to the allow side; reading a broker setting; decoding Avro or Protobuf; masking or denying columns; adding a data system; raising a default limit.
- **Never:** write to a datastore by any route; send a datastore password to the VM; build a statement by joining strings; run `KEYS`, `CONFIG`, `CLIENT` or `SLOWLOG`; join a consumer group or commit an offset; return a credential-bearing catalog relation; log a secret.

## Success Criteria

1. `db_query` answers one `SELECT`, `EXPLAIN` or `SHOW`, bounded and typed, on both access modes with the same result; no other statement runs.
2. A write by any route (a data-modifying CTE, a second statement, `SELECT … INTO`, a denied function) leaves the database unchanged, and the refusal names the rule.
3. `db_schema` returns the catalog from fixed queries and never reads a credential-bearing relation.
4. `redis_command` runs only the allow-listed commands with checked arguments, replies are bounded, and `KEYS`, `CONFIG`, `CLIENT` and `SLOWLOG` are refused.
5. `kafka_topics`, `kafka_lag` and `kafka_messages` answer as described, and after any of them no new group or committed offset exists.
6. `datastore_health` returns figures and findings from the fixed thresholds without reading query text, keys or message contents.
7. Rows, cells, replies, messages, total size and time are bounded, and answers say when they were cut.
8. Every tunnel and remote command is in the session log, and no datastore password or planted secret appears anywhere.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Personal data in production rows.** Answers are bounded and recorded in the session log up to 64 KB. Should a connection be able to list `deny_tables` or `masked_columns` in the context file (an amendment to `SPEC-contexts.md`), or is bounding enough for the first release?
2. **SQL guard level.** A real parser plus a function and relation deny-list plus PostgreSQL's own read-only transaction. Right level, and is "no `params` on the remote path" acceptable?
3. **Redis allow-list.** Right set? In particular `KEYS`, `CONFIG`, `CLIENT` and `SLOWLOG` are excluded.
4. **Kafka decoding.** Text and JSON only, with no Avro, Protobuf or schema registry in the first release. Acceptable?
5. **Kafka client and the consumer-group guarantee.** The client crate is chosen at build time, and the "no group, no commit" behaviour of the console consumer is proved by a mandatory real-broker test. Accept that the spec's status records the result afterwards?
6. **Tunnel port exposure.** An open `127.0.0.1` port is reachable by other local users on a shared machine. Acceptable as documented, or should Redis tunnels require a password?
