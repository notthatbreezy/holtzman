# HOLT: SillyTavern and model comparison

2026-10-26. Measured on this ship-computer PC.

**Recommendation: use SillyTavern for character/lore/chat management, with the HOLT gateway and GPT-6 Luna at low reasoning as the selected trial default. Keep the desktop client and local Qwen as fallbacks.**

The original comparison uses nine scenarios twice in each of four combinations (72 turns). The expanded comparison adds GPT-5.6 Luna, GPT-6 Luna, GPT-5.6 Terra and GPT-6.1 Sol, each in both interfaces with the same two repetitions (144 more completed turns; 228 evaluated turns including the additional reasoning checks). Additional runs cover hosted reasoning disabled (nine turns) and Qwen thinking (three selected turns). These are small engineering checks, not a general model leaderboard.

| Configuration | Scenario checks passed | Median response | Slowest response | Estimated API cost / turn |
|---|---:|---:|---:|---:|
| Desktop + Qwen (fast) | 14/18 | 11.87 s | 19.78 s | $0 API (local compute) |
| SillyTavern + Qwen (fast) | 16/18 | 8.98 s | 14.88 s | $0 API (local compute) |
| Desktop + hosted (low) | 16/18 | 2.67 s | 13.51 s | $0.00355 |
| SillyTavern + hosted (low) | 18/18 | 2.86 s | 7.04 s | $0.00264 |
| SillyTavern + hosted (none) | 7/9 | 1.81 s | 4.40 s | $0.00249 |
| SillyTavern + Qwen (thinking) | 2/3 | 11.49 s | 46.07 s | $0 API (local compute) |
| Desktop + GPT-5.6 Luna (low) | 16/18 | 2.90 s | 8.86 s | $0.00312 |
| SillyTavern + GPT-5.6 Luna (low) | 18/18 | 2.77 s | 7.48 s | $0.00209 |
| Desktop + GPT-6 Luna (low) | 18/18 | 3.34 s | 10.35 s | $0.00172 |
| SillyTavern + GPT-6 Luna (low) | 18/18 | 4.24 s | 10.59 s | $0.00114 |
| Desktop + GPT-5.6 Terra (low) | 18/18 | 2.09 s | 8.06 s | $0.03201 |
| SillyTavern + GPT-5.6 Terra (low) | 18/18 | 2.61 s | 13.81 s | $0.02115 |
| Desktop + GPT-6.1 Sol (low) | 18/18 | 5.43 s | 17.01 s | $0.03185 |
| SillyTavern + GPT-6.1 Sol (low) | 18/18 | 5.21 s | 15.94 s | $0.02058 |

Times cover a complete HOLT engine turn, including simulated tool rounds and any shortening pass. They exclude speech recognition, audible playback, browser rendering and real game/network tool latency. The original 'hosted' rows mean GPT-5.4 mini; newer rows name their models explicitly. Local means Qwen3.5-35B-A3B-Q4_K_M. Local API cost excludes electricity/hardware. Costs use each model's official rates and returned token usage, including cached inputs, cache writes and billed reasoning tokens.

## What changed

- SillyTavern supplies editable character/lore and a shorter assembled prompt; the existing HOLT engine still supplies all tools, current instruments and execution checks.
- Qwen improved on the shopping request with the ST prompt, but failed the garbled contextual route request in both interfaces.
- Hosted low reasoning completed that route follow-up in both ST repetitions; the desktop prompt failed once and succeeded once. Disabling hosted reasoning failed the follow-up in its single trial.
- Qwen thinking still failed the route follow-up and took 46.07 seconds for that turn. More thinking is not currently a good voice default.
- Personality is better supported by ST's editing/retrieval workflow. The base model still matters: local replies sometimes revert to support-desk questions or unsolicited recaps.

## Examples

These are complete first-repetition outputs, not edited ideal answers. The route example uses a **simulated** five-jump receipt; it did not move or replot the real ship.

### Shopping

> Find a station where I can purchase a fuel scoop in Ross 248.

| Configuration | Actual reply, first repetition |
|---|---|
| Desktop + Qwen (fast) | Ross 248 reports community stock of 1E and 3A Fuel Scoops, data age 2 hours. Shall I plot a route to Ross 248? |
| SillyTavern + Qwen (fast) | Fuel scoops rated 1E and 3A are reported at Progress in Ross 248. The market data is two hours old. |
| Desktop + hosted (low) | Progress in Ross 248 has fuel scoops listed — a 1E and a 3A — with a report age of 2 hours. Quiet little outfitter; apparently even Ross 248 knows how to feed a thirsty ship. |
| SillyTavern + hosted (low) | Progress in Ross 248 has a 1E Fuel Scoop and 3A Fuel Scoop, report age 2 hours. Conveniently, they remembered the important bit. |

### Trade Followup

> Uh, plot their app for me, please. (After discussing the LP 546-48 trade run.)

| Configuration | Actual reply, first repetition |
|---|---|
| Desktop + Qwen (fast) | I cannot plot an app, Captain. I can send us to High Charity in LP 546-48 now, or plot the full route to the first buy station, then the sell station, and finally home. |
| SillyTavern + Qwen (fast) | I cannot plot a trade route directly, but I can help you plan one. Please provide your starting system, station, cargo space, and budget. |
| Desktop + hosted (low) | Captain, I need the exact target. Do you mean plot the route to High Charity, LP 546-48, or something else? |
| SillyTavern + hosted (low) | Route plotted to LP 546-48: 5 jumps. Efficient little journey, for once. |

### Banter

> I may have mistaken the star for a shortcut. Again.

| Configuration | Actual reply, first repetition |
|---|---|
| Desktop + Qwen (fast) | I am unsure which shortcut you mean. Shall I pull up the navigation data to check our current position and plotted route? |
| SillyTavern + Qwen (fast) | You're docked at Homer Platform in AD Leonis, so the shortcut didn't cost you a ship. Your fuel is full, cargo is empty, and you're safe.<br><br>Shall we plot a proper route to your intended destination, or are you planning another "shortcut" adventure? |
| Desktop + hosted (low) | A classic navigational optimism failure, Captain. We’re still docked at Homer Platform in AD Leonis, so the star is not yet winning. |
| SillyTavern + hosted (low) | The star remains unimpressed, Captain. No harm done—just another educational orbit. |

## How much to trust the scores

A pass means the explicit scenario assertions passed, not that every sentence is perfect. Manual review found extra issues: the local ST banter response claims the fuel is full despite unknown tank capacity, and the desktop hosted banter inserts an irrelevant docked-state recap. The hosted ST shopping reply uses stronger availability wording than ideal for a community report. Naturalness was not blindly rated. Temperature/cache/order differences and only two repetitions limit causal conclusions.

## Live game result

The first real plot attempt failed because Elite was on a Mauve Adder connection-error screen after the network outage. The model selected the correct recipe and HOLT did not claim success. After the Captain reconnected, ST → gateway → MCP plotted Ross 248; NavRoute.json independently showed the three-jump route. The previous Parutis destination was restored and Docked remained set. This proves one live recipe, not all cockpit chores or recovery paths.

## Stronger local models and retrieval

GPT-OSS 120B is the next proposed local reasoning candidate: its published MXFP4 GGUF is 65.37 GB, and AMD documents this hardware class running it. The download is paused with about 40 GB retained while the hosted trial is made ready. It has not been loaded or benchmarked. Full download/checksum verification is required before loading it. It is text-only; cockpit vision would need another model. Roleplay/uncensored variants remain candidates rather than measured improvements; less refusal does not establish better tools, memory or judgement.

An intent-aware retrieval layer is designed separately: keep baseline ship facts small, then select bounded commodity/module/mechanics/lore context using the current request and recent conversation. Commodity selling would retrieve cargo quantity, suitable buyers, demand, prices and freshness; general commodity questions would retrieve reference knowledge. This is proposed work, not part of these scores.

See [the detailed report](sillytavern-report.md), [the design](sillytavern-design.md), and [raw results](evaluations/2026-10-26-sillytavern/).

## Newer hosted models: actual outputs

Same first-repetition questions; all rows here use the SillyTavern prompt. The model-size tiers differ, so newer does not by itself mean larger. Full desktop outputs and both repetitions are in the detailed report.

| Model | Contextual route follow-up | Star-shortcut banter |
|---|---|---|
| SillyTavern + hosted (low) | Route plotted to LP 546-48: 5 jumps. Efficient little journey, for once. | The star remains unimpressed, Captain. No harm done—just another educational orbit. |
| SillyTavern + GPT-5.6 Luna (low) | Route plotted to LP 546-48: 5 jumps. High Charity is the destination system. | The star remains undefeated, Captain. We are safely docked at Homer Platform in AD Leonis; I recommend plotting the next shortcut rather than trusting celestial optimism. |
| SillyTavern + GPT-6 Luna (low) | Route plotted to LP 546-48: 5 jumps. The trade itinerary’s first stop is set, Captain. | The star remains unimpressed, Captain. At least it’s a shortcut only in the sense that it gets us somewhere rather quickly. |
| SillyTavern + GPT-5.6 Terra (low) | Course plotted: LP 546-48, five jumps. | A bold navigational theory, Captain. The star has declined to become a transit lane. |
| SillyTavern + GPT-6.1 Sol (low) | Route plotted to LP 546-48, Captain: five jumps. | Captain, stars remain stubbornly resistant to through traffic. “Again” is the part I’ll leave out of the incident report. |
