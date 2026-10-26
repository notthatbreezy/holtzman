"""Small, grounded itinerary context survives conversational follow-up turns."""
def route_summary(data):
    legs = []
    for leg in data.get("legs", []):
        def port(key):
            return {k: leg.get(key, {}).get(k) for k in ("station", "system", "market_age_hours")}
        cargo = [{"commodity": c.get("name"), "tonnes": c.get("amount"),
                  "buy_price_per_tonne": c.get("source_commodity", {}).get("buy_price"),
                  "sell_price_per_tonne": c.get("destination_commodity", {}).get("sell_price")}
                 for c in leg.get("commodities", [])]
        legs.append({"buy_at": port("source"), "sell_at": port("destination"),
                     "cargo": cargo, "estimated_profit_credits": leg.get("total_profit", 0)})
    return {"itinerary": legs, "estimated_total_profit_credits": sum(l["estimated_profit_credits"] for l in legs),
            "next_system": legs[0]["sell_at"]["system"] if legs else None,
            "note": "Read-only community-data plan, not authority to act. Preserve both outbound and return legs."}
