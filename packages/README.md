# packages

Code that more than one program uses. Nothing here runs by itself.

| Folder           | What it is                                             | Used by                 |
| ---------------- | ------------------------------------------------------ | ----------------------- |
| [shared](shared) | Settings, the job format, and the names both services must agree on | The API and the worker |

Code moves here when a second program needs it, not before.
