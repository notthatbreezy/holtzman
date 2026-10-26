"""Read-only module stock lookup using the public Spansh station search."""
from datetime import datetime, timezone

SCHEMA = {
    "name": "find_module",
    "description": "Find stations reporting a ship module for sale IN the specified exact system. This is outfitting, not the commodity market. Excludes fleet carriers and planetary ports; requires a large pad. Reports community stock, timestamps and prices, not guaranteed live availability. Sends the supplied system and module name to Spansh. No game input or purchases.",
    "inputSchema": {"type": "object", "properties": {
        "system": {"type": "string", "minLength": 1, "maxLength": 128},
        "module": {"type": "string", "minLength": 1, "maxLength": 128},
        "module_class": {"type": "integer", "minimum": 1, "maximum": 8},
        "rating": {"type": "string", "enum": list("ABCDE")}},
        "required": ["system", "module"], "additionalProperties": False}}


def search_body(system, module, module_class=None, rating=None):
    match = {"name": module}
    if module_class is not None:
        match["class"] = [module_class]
    if rating:
        match["rating"] = [rating]
    return {"reference_system": system, "filters": {
        "distance": {"min": 0, "max": .1}, "has_large_pad": {"value": True},
        "is_planetary": {"value": False}, "has_outfitting": {"value": True},
        "modules": [match]}, "sort": [{"distance_to_arrival": {"direction": "asc"}}],
        "size": 100, "page": 0}


def select_results(data, system, module, module_class=None, rating=None):
    if data.get("reference", {}).get("name", "").casefold() != system.casefold():
        raise ValueError("Provider resolved a different system; check its exact name")
    stations = []
    for station in data.get("results", []):
        if (station.get("system_name", "").casefold() != system.casefold()
                or "carrier" in station.get("type", "").casefold()
                or not station.get("has_large_pad") or station.get("is_planetary")
                or not station.get("has_outfitting")):
            continue
        modules = [{k: m.get(k) for k in ("name", "class", "rating", "price")}
                   for m in station.get("modules", [])
                   if m.get("name", "").casefold() == module.casefold()
                   and (module_class is None or m.get("class") == module_class)
                   and (rating is None or m.get("rating") == rating)]
        if modules:
            stations.append({"station": station["name"], "system": system,
                             "arrival_ls": station.get("distance_to_arrival"), "large_pad": True,
                             "reported_at": station.get("outfitting_updated_at"), "modules": modules,
                             "source": "https://spansh.co.uk/station/" + str(station["id"])})
    stations.sort(key=lambda s: s["arrival_ls"] if s["arrival_ls"] is not None else float("inf"))
    return {"provider": "Spansh", "system": system, "module": module,
            "checked_at": datetime.now(timezone.utc).isoformat(), "stations": stations[:3],
            "note": "Community outfitting reports, not live stock. Verify in-game. Large-pad orbital stations only; fleet carriers excluded. No matches means none verified under those filters, not proof none exist."}


async def find_module(http, system, module, module_class=None, rating=None):
    response = await http.get("https://spansh.co.uk/api/stations/field_values/modules", timeout=20)
    response.raise_for_status()
    names = response.json()["values"]["name"]
    canonical = next((n for n in names if n.casefold() == module.strip().casefold()), None)
    if not canonical:
        raise ValueError("Unknown module name; use a standard name such as Fuel Scoop or Shield Generator")
    response = await http.post("https://spansh.co.uk/api/stations/search",
                               json=search_body(system.strip(), canonical, module_class, rating), timeout=25)
    response.raise_for_status()
    return select_results(response.json(), system.strip(), canonical, module_class, rating)
