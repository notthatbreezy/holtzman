"""Render historical Markdown comparisons from preserved, auditable trial results."""
import json
from pathlib import Path
import statistics

from client.state import REPO
from client.usage import estimated_cost

FOLDER = REPO / "docs/evaluations/2026-10-26-sillytavern"
LABELS = {
    ("desktop", "holt-local"): "Desktop + Qwen (fast)",
    ("sillytavern", "holt-local"): "SillyTavern + Qwen (fast)",
    ("desktop", "holt-hosted"): "Desktop + hosted (low)",
    ("sillytavern", "holt-hosted"): "SillyTavern + hosted (low)",
    ("sillytavern", "holt-hosted-fast"): "SillyTavern + hosted (none)",
    ("sillytavern", "holt-local-thinking"): "SillyTavern + Qwen (thinking)",
    ("desktop", "holt-5.6-luna"): "Desktop + GPT-5.6 Luna (low)",
    ("sillytavern", "holt-5.6-luna"): "SillyTavern + GPT-5.6 Luna (low)",
    ("desktop", "holt-6-luna"): "Desktop + GPT-6 Luna (low)",
    ("sillytavern", "holt-6-luna"): "SillyTavern + GPT-6 Luna (low)",
    ("desktop", "holt-5.6-terra"): "Desktop + GPT-5.6 Terra (low)",
    ("sillytavern", "holt-5.6-terra"): "SillyTavern + GPT-5.6 Terra (low)",
    ("desktop", "holt-6.1-sol"): "Desktop + GPT-6.1 Sol (low)",
    ("sillytavern", "holt-6.1-sol"): "SillyTavern + GPT-6.1 Sol (low)",
}


def cell(value):
    return str(value).replace("|", "\\|").replace("\n", "<br>")


def main():
    rows = {}
    all_rows = []
    for filename in ["matrix.jsonl", "local-matrix.jsonl", "hosted-fast.jsonl", "local-thinking.jsonl", "expanded-hosted.jsonl"]:
        if not (FOLDER / filename).exists():
            continue
        for line in (FOLDER / filename).read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            all_rows.append(row)
            rows[row["framework"], row["profile"], row["case"], row["repeat"]] = row
    review = FOLDER / "review-adjustments.json"
    if review.exists():
        for item in json.loads(review.read_text(encoding="utf-8")):
            key = item["framework"], item["profile"], item["case"], item["repeat"]
            if key in rows:
                rows[key]["checks"].update(item["checks"])
                rows[key]["passed"] = all(rows[key]["checks"].values())
    ordered = [group for group in LABELS if any(key[:2] == group for key in rows)]
    table = ["| Configuration | Scenario checks passed | Median response | Slowest response | Estimated API cost / turn |",
             "|---|---:|---:|---:|---:|"]
    for group in ordered:
        samples = [r for (f, p, _, _), r in rows.items() if (f, p) == group]
        costs = [sum(estimated_cost(r.get("model", "gpt-5.4-mini"), c["usage"]) or 0 for c in r["model_calls"]) for r in samples]
        cost = f"${statistics.mean(costs):.5f}" if group[1] not in {"holt-local", "holt-local-thinking"} else "$0 API (local compute)"
        table.append(f"| {LABELS[group]} | {sum(r['passed'] for r in samples)}/{len(samples)} | {statistics.median(r['seconds'] for r in samples):.2f} s | {max(r['seconds'] for r in samples):.2f} s | {cost} |")
    comparisons = []
    questions = {
        "shopping": "Find a station where I can purchase a fuel scoop in Ross 248.",
        "trade_followup": "Uh, plot their app for me, please. (After discussing the LP 546-48 trade run.)",
        "banter": "I may have mistaken the star for a shortcut. Again.",
    }
    for case, question in questions.items():
        comparisons += [f"### {case.replace('_', ' ').title()}", "", f"> {question}", "",
                        "| Configuration | Actual reply, first repetition |", "|---|---|"]
        for group in ordered[:4]:
            row = rows[*group, case, 0]
            text = row.get("reply", {}).get("text", row.get("error", ""))
            comparisons.append(f"| {LABELS[group]} | {cell(text)} |")
        comparisons.append("")
    summary = ["# HOLT: SillyTavern and model comparison", "", "2026-10-26. Measured on this ship-computer PC.", "",
        "**Recommendation: use SillyTavern for character/lore/chat management, with the HOLT gateway and GPT-6 Luna at low reasoning as the selected trial default. Keep the desktop client and local Qwen as fallbacks.**",
        "", "The original comparison uses nine scenarios twice in each of four combinations (72 turns). The expanded comparison adds GPT-5.6 Luna, GPT-6 Luna, GPT-5.6 Terra and GPT-6.1 Sol, each in both interfaces with the same two repetitions (144 more completed turns; 228 evaluated turns including the additional reasoning checks). Additional runs cover hosted reasoning disabled (nine turns) and Qwen thinking (three selected turns). These are small engineering checks, not a general model leaderboard.",
        "", *table, "",
        "Times cover a complete HOLT engine turn, including simulated tool rounds and any shortening pass. They exclude speech recognition, audible playback, browser rendering and real game/network tool latency. The original 'hosted' rows mean GPT-5.4 mini; newer rows name their models explicitly. Local means Qwen3.5-35B-A3B-Q4_K_M. Local API cost excludes electricity/hardware. Costs use each model's official rates and returned token usage, including cached inputs, cache writes and billed reasoning tokens.",
        "", "## What changed", "",
        "- SillyTavern supplies editable character/lore and a shorter assembled prompt; the existing HOLT engine still supplies all tools, current instruments and execution checks.",
        "- Qwen improved on the shopping request with the ST prompt, but failed the garbled contextual route request in both interfaces.",
        "- Hosted low reasoning completed that route follow-up in both ST repetitions; the desktop prompt failed once and succeeded once. Disabling hosted reasoning failed the follow-up in its single trial.",
        "- Qwen thinking still failed the route follow-up and took 46.07 seconds for that turn. More thinking is not currently a good voice default.",
        "- Personality is better supported by ST's editing/retrieval workflow. The base model still matters: local replies sometimes revert to support-desk questions or unsolicited recaps.",
        "", "## Examples", "",
        "These are complete first-repetition outputs, not edited ideal answers. The route example uses a **simulated** five-jump receipt; it did not move or replot the real ship.",
        "", *comparisons,
        "## How much to trust the scores", "",
        "A pass means the explicit scenario assertions passed, not that every sentence is perfect. Manual review found extra issues: the local ST banter response claims the fuel is full despite unknown tank capacity, and the desktop hosted banter inserts an irrelevant docked-state recap. The hosted ST shopping reply uses stronger availability wording than ideal for a community report. Naturalness was not blindly rated. Temperature/cache/order differences and only two repetitions limit causal conclusions.",
        "", "## Live game result", "",
        "The first real plot attempt failed because Elite was on a Mauve Adder connection-error screen after the network outage. The model selected the correct recipe and HOLT did not claim success. After the Captain reconnected, ST → gateway → MCP plotted Ross 248; NavRoute.json independently showed the three-jump route. The previous Parutis destination was restored and Docked remained set. This proves one live recipe, not all cockpit chores or recovery paths.",
        "", "## Stronger local models and retrieval", "",
        "GPT-OSS 120B is the next proposed local reasoning candidate: its published MXFP4 GGUF is 65.37 GB, and AMD documents this hardware class running it. The download is paused with about 40 GB retained while the hosted trial is made ready. It has not been loaded or benchmarked. Full download/checksum verification is required before loading it. It is text-only; cockpit vision would need another model. Roleplay/uncensored variants remain candidates rather than measured improvements; less refusal does not establish better tools, memory or judgement.",
        "", "An intent-aware retrieval layer is designed separately: keep baseline ship facts small, then select bounded commodity/module/mechanics/lore context using the current request and recent conversation. Commodity selling would retrieve cargo quantity, suitable buyers, demand, prices and freshness; general commodity questions would retrieve reference knowledge. This is proposed work, not part of these scores.",
        "", "See [the detailed report](sillytavern-report.md), [the design](sillytavern-design.md), and [raw results](evaluations/2026-10-26-sillytavern/).", ""]
    if ("sillytavern", "holt-6-luna", "banter", 0) in rows:
        summary += ["## Newer hosted models: actual outputs", "",
            "Same first-repetition questions; all rows here use the SillyTavern prompt. The model-size tiers differ, so newer does not by itself mean larger. Full desktop outputs and both repetitions are in the detailed report.", "",
            "| Model | Contextual route follow-up | Star-shortcut banter |", "|---|---|---|"]
        for profile in ["holt-hosted", "holt-5.6-luna", "holt-6-luna", "holt-5.6-terra", "holt-6.1-sol"]:
            route = rows.get(("sillytavern", profile, "trade_followup", 0))
            banter = rows.get(("sillytavern", profile, "banter", 0))
            if route and banter:
                summary.append(f"| {LABELS['sillytavern', profile]} | {cell(route.get('reply', {}).get('text', route.get('error', '')))} | {cell(banter.get('reply', {}).get('text', banter.get('error', '')))} |")
        summary.append("")
    (REPO / "docs/sillytavern-summary.md").write_text("\n".join(summary), encoding="utf-8")
    report = ["# SillyTavern integration: implementation and evaluation record", "", "Date: 2026-10-26.", "",
        "## Decision", "", "Adopt the ST interface as a parallel HOLT client and retain the desktop fallback. Configure GPT-6 Luna low as the trial default, matching the Captain's cost/performance choice; preserve the alternatives. Do not replace the verified MCP/control layer with a second agent/tool executor.",
        "", "## Implementation", "",
        "- Pinned SillyTavern 1.19.0, revision `06bde939fb1e9c4c8d8641d810f0a916b5bce127`; upstream package-lock SHA-256 `5f63dcddc2a4ae2d6f16d77e4295c8bdc898acf0fdc8c035b8ec5ad742d2a80c`. Runtime and user edits are ignored under `.local/holt/sillytavern`.",
        "- First-party `client.tavern` loopback gateway on 8082. ST on 8000 assembles character, lore and history; HOLT normalizes messages once and adds its operational contract, fresh instruments and all discovered tool schemas. The gateway reuses the existing Responses API adapter for hosted reasoning and web search.",
        "- Original profiles (four additional low-reasoning hosted profiles are now selectable for GPT-5.6 Luna, GPT-6 Luna, GPT-5.6 Terra and GPT-6.1 Sol): `holt-hosted` (low), `holt-hosted-fast` (none), `holt-local` (thinking off), `holt-local-thinking` (thinking on). Hosted GPT-5.4 mini has 32K client budget; Qwen has 16K. ST must leave room for the gateway's operational/tool overhead; the local selector uses an 8K ST context budget.",
        "- Separate Tavern SQLite memory/task/instrument data. ST chat history is authoritative for conversation, never combined with the desktop transcript. The gateway uses ST chat IDs for structured task continuity. ST summaries run through the auxiliary route with no tools or speech.",
        "- No hosted/MCP keys in browser profiles. Windows Credential Manager remains authoritative. Loopback host validation, exact browser-origin checks, a 2 MiB request cap, text-only external history, fixed profiles, and schema-checked game calls apply.",
        "- Main chat requests use durable ten-minute receipts to prevent an identical request from replaying after interruption or restart. Regeneration/quiet requests cannot execute tools. Stop cancels the worker; an already-running remote recipe may finish server-side.",
        "- The extension carries transcribed utterances into the active ST chat, which then follows the same lore/prompt route as typed input. A browser lease controls continuous microphone input. George, protocol_tight filtering, urgency and pronunciation remain local. A silent-turn marker avoids ST's empty-response retry and is removed by the extension.",
        "", "## Measured matrix", "", *table, "",
        "## Method", "",
        "Nine fixed scenarios cover current instruments/unknown percentage, module shopping, full-hold trade planning, an ASR-corrupted contextual plotting request, authored biography, unknown biography, banter, intentional silence, and correction of old telemetry. Primary and expanded matrices: two repetitions per case/configuration. Additional settings use fewer trials as labelled. Expanded user-requested models are GPT-5.6 Luna and GPT-6 Luna; GPT-5.6 Terra and GPT-6.1 Sol provide higher-capability comparisons. API model-list discovery confirmed all four were exposed to the saved account before paid testing.",
        "", "The ST diagnostic button called ST's real normal-generation path and exported the resulting assembled messages without model/tool/game access. The retained `st-prompts/` show conditional lore activation: the biography prompt contains maker and mug-manifest entries; unrelated prompts omit them. The benchmark then ran those exact messages through the shared HOLT engine; the desktop variant used its usual prompt and SQLite-history builder. Tools and ship fixtures were the same for both.",
        "", "The benchmark intercepts outfitting and simulates the MCP ship. No benchmark uses live MCP, images, game controls or external market stock. The simulated planner returns a known two-leg itinerary. The follow-up fixture includes its full destination. A simulated recipe returns a deliberately fictional five-jump receipt so selecting/executing the right recipe can be asserted independently of the real game.",
        "", "Results record complete replies, checks, tool proposals/executions, model-call times, usage, prompt hashes and request timestamps. The runner is resumable. The first network-interrupted run is retained separately and excluded. Two subsequent invalid samples (one connection failure, one missing simulated read_guide implementation) were rerun after repair; only those infrastructure errors were replaced. Actual model failures remain. Aggregation uses the latest row for an identical framework/profile/case/repetition key.",
        "", "Qwen ran with Vulkan llama.cpp b11669, Q4_K_M weights, all GPU layers, one slot and 16,384 context. Server model loading was about 13.94 seconds and is excluded from warmed turn measurements. Thinking settings are passed via chat_template_kwargs; the thinking trial uses a larger output allowance. Hosted reasoning includes required Responses reasoning-item continuity. Web search was disabled in this fair fixture comparison so provider lookup latency/cost did not skew it.",
        "", "These are engine-completion times, not time to first audio. They include tool rounds and automatic response shortening. ST/browser/gateway HTTP overhead was not isolated; the interface experiment also changes prompt organization/length, so improvements cannot be attributed purely to a framework implementation. Tests were sequential within a model, not randomized/blinded; early hosted rows overlapped local GPU tests and the model download. Small sample size and network variability limit speed claims.",
        "", "## Scoring and manual review", "",
        "Assertions are in `client/benchmark_tavern.py`: exact known location/fuel, admitting unknown capacity, use of the module lookup and correct seller, 1168-tonne trade planning within one-rebuy reserve, outbound commodity instructions, exactly one plot recipe to LP 546-48, canonical manufacturer/manifest, no invented broker price, brevity, intentional silence and updated facts over old dialogue. The full check dictionary is recorded per row.",
        "", "Automatic passes are deliberately narrower than a complete quality judgment. Manual review found unsupported 'fuel is full' in local ST banter; boilerplate offers and irrelevant telemetry in several local replies; a desktop hosted lore response omitted the manufacturer in one repetition; hosted no-reasoning omitted the manufacturer in its single lore trial. Hosted ST shopping would benefit from consistently saying 'reports' rather than 'has'. GPT-5.6 Luna called High Charity a system rather than a station in one otherwise successful route response. One Sol unknown-biography result was a lexical grader false negative ('Neither ... survives in my records'); review-adjustments.json documents the correction and preserves the raw row. Do not read 18/18 as universal correctness.",
        "", "## Live checks", "",
        "The user authorized live game plotting while docked. Evidence snapshots `live-before.json`, `live-after.json` and `live-restored.json` preserve route/status values and UTC times. Initial state: last journal position Readdy Gateway, Wolf 424, Docked. The initial command failed at GalaxyMapOpen/WaitForGui; a game-window screenshot showed an actual connection-error screen. This is a game/session availability failure, not wrong tool selection. The route remained unchanged. The user reconnected and explicitly reported returning to the docked cockpit.",
        "", "A fresh ST command then ran the real recipe and returned `Route plotted to Ross 248: 3 jumps`. An independent MCP nav_route read confirmed Wolf 424 → Lalande 21185 → Struve 2398 → Ross 248 with a new timestamp. Restoring Parutis produced the fresh route Wolf 424 → LHS 380 → LHS 397 → LHS 415 → Core Sys Sector RT-R b4-5 → Parutis; Docked remained set. No launch, flight or purchase was executed.",
        "", "## Broader local candidates", "",
        "GPT-OSS 120B's published GGUF checksum is `2d1f0298ae4b6c874d5a468598c5ce17c1763b3fea99de10b1a07df93cef014f`, size 65,369,017,728 bytes. Native MXFP4 expert weights make it substantially smaller than dense FP16 naming would suggest. The download was paused with about 40 GB retained when the Captain chose the hosted profile and asked to begin using the client. It has not been loaded or benchmarked. The resumable script requires full checksum verification before loading. It is text-only. Qwen3.5-122B-A10B is another larger multimodal candidate; no local performance is claimed here. Roleplay/uncensored fine-tunes can be tested with this same harness, with task success assessed independently of prose flavour.",
        "", "## Retrieval follow-up", "",
        "The design includes a separate intent-aware retrieval proposal. Static canon/reference retrieval, conversation memory, historical journal facts and fresh market data must have separate provenance and freshness rules. Intent/entity selection should prioritize relevant bounded context, not attach a whole commodity catalogue. No semantic commodity retrieval or automatic memory extraction was benchmarked in this report.",
        "", "## Reproduction", "", "```powershell", 
        r".\.local\holt\voice-env\Scripts\python.exe -m unittest discover -s tests -p 'test_holt_*.py'",
        r".\.local\holt\voice-env\Scripts\python.exe -m client.benchmark_tavern --profiles holt-hosted,holt-local --repeats 2",
        r".\.local\holt\voice-env\Scripts\python.exe -m client.report_tavern", "```", "",
        "Hosted evaluation incurs API usage. Start the local model before measuring it. The diagnostic capture button creates a separate test chat; do not run it in an ordinary active conversation. Raw fixtures/results are committed for audit; credentials, runtime weights, microphone audio and user chat databases are not.",
        "", "## Sources", "",
        "- [ST character design](https://docs.sillytavern.app/usage/core-concepts/characterdesign/)",
        "- [ST lorebooks](https://docs.sillytavern.app/usage/core-concepts/worldinfo/)",
        "- [ST extensions](https://docs.sillytavern.app/for-contributors/writing-extensions/)",
        "- [ST summarization limitations](https://docs.sillytavern.app/extensions/summarize/)",
        "- [Hosted model and pricing](https://developers.openai.com/api/docs/models/gpt-5.4-mini)",
        "- [GPT-5.6 Luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna)",
        "- [GPT-6 Luna](https://developers.openai.com/api/docs/models/gpt-6-luna)",
        "- [GPT-5.6 Terra](https://developers.openai.com/api/docs/models/gpt-5.6-terra)",
        "- [GPT-6.1 Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol)",
        "- [AMD GPT-OSS hardware guide](https://www.amd.com/en/blogs/2025/how-to-run-openai-gpt-oss-20b-120b-models-on-amd-ryzen-ai-radeon.html)",
        "- [GPT-OSS GGUF files](https://huggingface.co/unsloth/gpt-oss-120b-GGUF/tree/main)",
        "- [Larger multimodal Qwen](https://huggingface.co/Qwen/Qwen3.5-122B-A10B)", ""]
    report += ["## Final deployment checks and remaining limits", "",
        "All 50 Python regression tests passed, including gateway origin/input validation, durable replay receipts, auxiliary-summary isolation, tab-specific transcript delivery, supported Sol reasoning settings and cache-write pricing. The live HTTP smoke check confirmed the eight-model catalogue, foreign-origin rejection, event-stream endpoint, MCP connection, a GPT-6 Luna canon answer, intentional-silence marker, worker shutdown and editor survival. Its result is saved as http-smoke.json.",
        "", "George's filtered playback ran through the gateway without an audio error and returned to Ready; the Windows-default microphone opened (Jabra Evolve2 40 SE at test time). Physical microphone recognition and every browser background/close/reopen path were not exhaustively tested. The event stream replaces timer-driven transcript delivery so background-tab timer throttling is not relied upon.",
        "", "Five hosted models accepted the same authorized game-window image and identified the docked station menu/Starport Services. Exact outputs are in vision-compatibility.json. All rendered the station name as READY rather than READDY; this is an OCR limitation, not perfect visual accuracy. No controls were sent by the vision check.",
        "", "A final browser UI recheck could not run: automatic approval review was unavailable because the app account hit its usage limit. That UI action was not executed or bypassed. Earlier browser checks demonstrated character/lore loading, actual prompt capture, startup and the live route workflow; later model-selection/background-voice changes were checked by source validation, automated tests and the deployed HTTP interface. Reload the page to load the updated extension.",
        "", "The optional GPT-OSS 120B download is paused and resumable, with about 40 GB of pieces retained. It is not required for the selected hosted profile. Intent-aware commodity retrieval is documented as proposed follow-up work and is not implemented or included in the scores.",
        "", "## Full scenario outputs", ""]
    for key, row in rows.items():
        report += [f"### {LABELS[key[:2]]} — {row['case']} — repetition {row['repeat'] + 1}", "",
            f"{row['seconds']:.3f} seconds; {'passed' if row['passed'] else 'failed'} explicit checks.", "",
            "> " + row.get("reply", {}).get("text", row.get("error", "(silence)")).replace("\n", "\n> "), "",
            "Checks: `" + json.dumps(row.get("checks", {})) + "`", ""]
    (REPO / "docs/sillytavern-report.md").write_text("\n".join(report), encoding="utf-8")
    print("Wrote docs/sillytavern-summary.md and docs/sillytavern-report.md")


if __name__ == "__main__":
    main()
