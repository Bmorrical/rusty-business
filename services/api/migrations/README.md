# services/api/migrations

The database's history. Each file is one change, written in SQL, and they
run in number order.

| File                | What it does                                                   |
| ------------------- | -------------------------------------------------------------- |
| `0001_starter.sql`  | Makes `bootstrap_notes` with one row. It proves the path from this folder to a running database |

How they run: the files are built into the API when it compiles, and the API
runs any that a database has not seen yet each time it starts. There is no
separate migrate command.

Rules:

- **Forward only.** A change to the database is a new file with the next
  number: `0002_what_it_does.sql`. There are no "down" files.
- **Never edit a file that has run anywhere.** The API keeps a checksum of
  each one and refuses to start if a file it already ran has changed.
- Only files named `NNNN_name.sql` are read. This README is not.
