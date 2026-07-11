# Linux ABI — running foreign binaries

preon runs unmodified Linux binaries the microkernel way: not as a syscall
layer bolted into the kernel, but as a userspace **personality server**. A
process tagged `linux` has its syscalls shifted and routed over IPC to the
Linux ABI server, which translates them into preon capability operations and
speaks to the native VFS on the process's behalf. None of it touches the
privileged core — the same isolation every userspace server gets.

This document covers the part that is more than a name-mapping: giving a
Linux program a *usable* view of the filesystem, and the concrete kernel-
behaviour it must fake to feel native. For the design intent see
[`IDEA.md`](IDEA.md) (the personality model and the namespaced VFS); this is
the working list of what the Linux personality actually has to solve.

---

## Full citizens, not prisoners

The naive approach — root a Linux process at a private `/compat/linux`
subtree and stop there — traps it. Its `/lib`, `/etc`, `/proc` resolve
inside the sandbox, but it cannot open your native source at
`/home/you/projects/src/main.rs`; that path does not exist in its world. An
editor like Helix launched under the personality would be useless.

Two mechanisms, used together, make a Linux program a first-class citizen of
the filesystem while keeping the sandbox:

### 1. Bind mounts — splice native subtrees into the Linux namespace

Because a namespace is just a table of prefixes → server endpoints (see
[`IDEA.md`](IDEA.md)), the root task can assemble the Linux process's
namespace from *both* the `/compat/linux` tree and chosen native subtrees:

```
/                     -> /compat/linux        (the Linux root)
/proc                 -> lx_procfs            (synthetic Linux /proc)
/dev                  -> lx_devfs             (/dev/null, /dev/zero, …)
/home/you/projects    -> native RootFS server (a real preon directory)
```

When Helix walks into `/home/you/projects`, the VFS routes the request to the
**native** filesystem server. Helix believes it is on an ordinary Linux disk;
in fact it is talking to preon's own VFS over IPC. It edits the file, saves,
and native tools see the change immediately — the bytes only ever lived in
one place.

### 2. Path translation in the ABI server

Some binaries hard-code absolute paths that live elsewhere on preon. Before
forwarding an `open()`, the ABI server runs the path through a translation
map (the trick WSL uses for `/mnt/c/...` ↔ `C:\...`):

- **System paths** (`/bin`, `/lib`, `/etc`) are rewritten into the sandbox:
  `/lib/...` → `/compat/linux/lib/...`.
- **User paths** that correspond to a bound-in native subtree pass through
  unchanged (or are rewritten to their native form) — those the VFS resolves
  to the native server directly.

### The upshot

A Linux program is a full citizen **by capability**: it sees and edits
exactly the native files its namespace was granted — bounded not by a wall
but by the same capability rules as everything else. The architect chooses,
per personality, what to expose: bind the workspace and the network endpoint
but not the GPU port, and the Linux process simply *cannot* draw to the
screen, no policy check required. Native speed, native file access, native
isolation.

---

## Challenges to solve

The ABI server is a "great impersonator": it must not merely move bytes, it
must **simulate Linux kernel behaviour** over a foreign VFS. These are the
concrete tasks to design before the Linux personality is real. They are open
problems, listed here so they are not discovered the hard way.

- **Dynamic-linker path (`execve`).** Linux ELFs are rarely static; they name
  a hard-coded interpreter (`/lib/ld-linux-aarch64.so.1`) that runs first. The
  ABI server must intercept `execve`, recognise a Linux ELF, and force the
  interpreter path into `/compat/linux/lib/...` regardless of what the binary
  requests — otherwise the loader is resolved against the wrong tree.

- **Metadata translation (`stat`).** Linux `stat` returns a fixed struct with
  Linux types: `uid`/`gid`, `dev_t`, and `atime`/`mtime`/`ctime` as
  seconds-since-1970. preon's native metadata may be capability-shaped and its
  time tick-based. The ABI server must pack native metadata into a
  Linux-compatible `struct stat`, synthesizing fields Linux insists on but
  preon lacks (e.g. a fake `uid = 1000`) — or tools like `ls` reject the file
  as broken.

- **Advisory locks (`fcntl` / `flock`).** Databases (SQLite) and editors take
  advisory locks to keep two processes off one file. A namespace-of-IPC-
  channels VFS may have no native lock concept, so locks are emulated — in the
  ABI server or the VFS — as a table of "process P holds region R of file X";
  a conflicting Linux locker gets `EAGAIN`.

- **Change notification (`inotify`).** Editors and watchers (`cargo watch`)
  rely on `inotify` to refresh when a file changes underneath them. The
  uniform file protocol should carry an `EV_FILE_CHANGED` event the VFS
  broadcasts to subscribers; the ABI server turns those native notifications
  into Linux `inotify` events. This wants to be in the protocol from day one,
  not retrofitted.

- **Open-then-`unlink` lifetime.** POSIX keeps an unlinked-but-open file alive
  until its last descriptor closes. A simpler "delete = close the channel,
  free the memory" VFS breaks that. The VFS needs **reference-counted file
  sessions**: the FS server does not reclaim blocks at `unlink`, only at the
  final `REQ_CLOSE`.

- **Path case-sensitivity.** Linux paths are case-sensitive (`File.rs` ≠
  `file.rs`). If a native preon filesystem ever chooses case-insensitive
  names — or a second personality (Windows) arrives — the ABI server must do
  case-folding lookups at the boundary. Only bites once such a filesystem
  exists.

---

## Why the microkernel makes this tractable

Every item above is a **userspace translation of error codes and structs**,
not a kernel change. Because the native VFS is pure IPC (Plan 9-style), the
Linux personality is a program you can write, debug under GDB, crash, and
restart — while the kernel and the native servers stay clean and unaware that
any of this Linux-compat machinery exists. The "great impersonation" is
contained entirely in one replaceable userspace server.
