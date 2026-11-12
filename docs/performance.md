# Performance

## postui, measured

[`scripts/bench.sh`](../scripts/bench.sh) runs the release binary in a
pseudo-terminal against a synthetic workspace (701 requests in 21
collections), drives it with scripted keys and samples the process with `ps`.
A local `python3 -m http.server` serves a ~5 MB JSON file, so sending doesn't
touch the network.

Results on an Apple-silicon Mac (macOS, release build, commit `6e6b579`):

| Step | RSS | CPU time (cumulative) |
|---|---|---|
| Startup | 13.1 MB | 0.03 s |
| Idle 10 s | 13.1 MB | 0.10% of one core |
| Collection browser + fuzzy search | 13.8 MB | 0.06 s |
| Switch tabs ×20 | 15.3 MB | 0.12 s |
| Send, ~5 MB JSON response | 135.6 MB | 0.22 s |
| Send again | 186.5 MB | 0.32 s |
| Scroll and search the response | 170.2 MB | 2.16 s |
| Idle 10 s afterwards | 175.5 MB | 0.10% of one core |

With a real 2.8 MB workspace (712 requests, large bodies) startup was 25 MB.
The release binary is about 7 MB.

What the numbers say:

- **Idle cost is close to zero.** The UI redraws only on input; the event loop
  wakes ten times a second to check for autosave, which costs about 0.1% of a
  core.
- **Large responses dominate memory.** The response viewer stores text as one
  `char` (4 bytes) per character, and a pretty-printed body is longer than the
  raw one, so a 5 MB JSON response grows memory by roughly 120 MB. Searching
  and jumping through such a buffer is also where most CPU time goes.
  Typical API responses (kilobytes) don't register.

Run `scripts/bench.sh` yourself to reproduce or compare (macOS and Linux;
needs `python3`, `ps` and `script`).

## Compared with GUI clients

We have not measured GUI clients on the same machine. Published figures vary
a lot with version, OS and how many collections are open, so treat them as
orders of magnitude rather than a benchmark:

| Client | Reported idle memory | Source |
|---|---|---|
| Postman (Electron) | ~200–600 MB, reports of 1 GB+ | GitHub issues [#6015](https://github.com/postmanlabs/postman-app-support/issues/6015), [#4687](https://github.com/postmanlabs/postman-app-support/issues/4687); comparison articles [1](https://yrkan.com/blog/api-tools-comparison-2025/), [2](https://www.devtoolreviews.com/reviews/api-testing-tools-postman-vs-insomnia-vs-bruno-vs-hoppscotch-2026) |
| Insomnia (Electron) | ~150–290 MB | [1](https://yrkan.com/blog/postman-vs-insomnia-comparison/), [2](https://www.devtoolreviews.com/reviews/api-testing-tools-postman-vs-insomnia-vs-bruno-vs-hoppscotch-2026) |
| Bruno (Electron) | ~100–180 MB | [1](https://yrkan.com/blog/api-tools-comparison-2025/), [2](https://www.devtoolreviews.com/reviews/api-testing-tools-postman-vs-insomnia-vs-bruno-vs-hoppscotch-2026) |
| postui | 13–25 MB | measured above |

Electron apps also run several processes (main, renderer, GPU); the figures
above may count only some of them. For a fair comparison on your machine,
measure the GUI client's total memory in Activity Monitor or with
`ps -o rss= -p <pids>` after opening the same collection.
