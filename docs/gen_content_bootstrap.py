#!/usr/bin/env python3
"""One-off author tool: emits the initial RON content under data/. After generation the RON files
are the source of truth and are edited by hand."""
import os, sys, math, random

ROOT = sys.argv[1]
OUT = os.path.join(ROOT, "data")
os.makedirs(os.path.join(OUT, "parts"), exist_ok=True)

# ---------------------------------------------------------------- RON emitter
class E:  # enum unit / tuple / struct variant
    def __init__(self, name, *args, **kw):
        self.name, self.args, self.kw = name, args, kw
class T(tuple):
    pass
class Raw(str):
    pass

def ron(v, ind=0):
    pad = "    " * ind
    if isinstance(v, Raw):
        return str(v)
    if v is True:
        return "true"
    if v is False:
        return "false"
    if v is None:
        return "None"
    if isinstance(v, E):
        if v.kw:
            inner = ", ".join(f"{k}: {ron(x, ind)}" for k, x in v.kw.items())
            return f"{v.name}({inner})"
        if v.args:
            return f"{v.name}(" + ", ".join(ron(a, ind) for a in v.args) + ")"
        return v.name
    if isinstance(v, T):
        return "(" + ", ".join(ron(a, ind) for a in v) + ")"
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        r = repr(round(v, 3))
        return r if "." in r or "e" in r else r + ".0"
    if isinstance(v, dict):
        items = [(k, x) for k, x in v.items() if x is not None]
        if not items:
            return "()"
        s = "(\n"
        for k, x in items:
            s += pad + "    " + f"{k}: {ron(x, ind + 1)},\n"
        return s + pad + ")"
    if isinstance(v, list):
        if not v:
            return "[]"
        parts = [ron(x, ind + 1) for x in v]
        if all(len(p) < 30 and "\n" not in p for p in parts) and sum(len(p) for p in parts) < 90:
            return "[" + ", ".join(parts) + "]"
        s = "[\n"
        for p in parts:
            s += pad + "    " + p + ",\n"
        return s + pad + "]"
    raise TypeError(v)

def write(name, v, header=""):
    with open(os.path.join(OUT, name), "w") as f:
        if header:
            f.write("".join("// " + l + "\n" for l in header.strip().split("\n")))
        f.write(ron(v) + "\n")

F = float
def v3(a, b, c):
    return T((F(a), F(b), F(c)))
def c3(r, g, b):
    return T((int(r), int(g), int(b)))

# ---------------------------------------------------------------- moves
# (id, name, kind, windup, active, recovery, dmg, posture, stamina, reach, radius, arc, lunge, extra)
MOVES = []
def mv(id, name, kind, w, a, r, d, p, s, reach=3.0, rad=2.0, arc=0, lunge=0.0, **x):
    m = dict(id=id, name=name, kind=E(kind), windup=w, active=a, recovery=r, damage=F(d), posture=F(p),
             stamina=F(s), reach=F(reach), radius=F(rad), arc=F(arc), lunge=F(lunge))
    if x.get("hyper"): m["hyper_armor"] = True
    if "proj" in x:
        sp, ra, life, grav, cnt, spr = (x["proj"] + (0, 1, 0))[:6] if len(x["proj"]) < 6 else x["proj"]
        pj = dict(speed=F(sp), radius=F(ra), life=int(life), gravity=F(grav), count=int(cnt), spread=F(spr))
        if x.get("homing"): pj["homing"] = F(x["homing"])
        m["projectile"] = pj
    if "fx" in x: m["effect"] = E(x["fx"])
    if "feint" in x: m["feint"] = x["feint"]
    m["cue"] = E(x.get("cue", "Swing"))
    m["telegraph"] = E(x.get("tele", "Raise"))
    m["avoid"] = E(x.get("avoid", "Any"))
    if x.get("unblockable"): m["blockable"] = False
    if "lore" in x: m["lore"] = x["lore"]
    MOVES.append(m)

# --- Saint player moves
mv("blade_slash", "Penitent Slash", "Light", 8, 6, 14, 22, 10, 12, 3.4, 2.2, 110, 1.2)
mv("blade_cleave", "Absolution Cleave", "Heavy", 20, 6, 22, 46, 30, 24, 3.8, 2.6, 150, 2.2)
mv("shield_bash", "Votive Bash", "Bash", 12, 5, 16, 10, 28, 14, 2.4, 2.2, 0, 1.5, cue="Clank", tele="Coil")
mv("bulwark_charge", "Bulwark Charge", "Special", 14, 10, 20, 20, 40, 30, 2.6, 2.6, 0, 9.0, hyper=True, cue="Charge", tele="Coil")
mv("flame_spray", "Kiln Spray", "Light", 8, 10, 10, 9, 4, 10, 4.6, 2.4, 30, 0, fx="Burn", cue="Hiss", tele="Coil")
mv("flame_gout", "Cinder Gout", "Heavy", 22, 14, 22, 30, 22, 28, 6.0, 3.0, 20, 0, fx="Burn", cue="Hiss")
mv("bellows_bash", "Bellows Bash", "Bash", 12, 5, 16, 12, 24, 14, 2.4, 2.2, 0, 1.2, cue="Clank", tele="Coil")
mv("napalm_mortar", "Napalm Mortar", "Special", 20, 3, 20, 28, 10, 34, 2.0, 2.6, proj=(20, 2.6, 90, 16, 1, 0), fx="Burn", cue="Charge")
mv("hook_jab", "Talon Jab", "Light", 6, 4, 10, 14, 8, 8, 3.8, 1.6, 20, 0.8, cue="Click")
mv("reel_strike", "Reel Strike", "Heavy", 16, 6, 18, 32, 26, 20, 8.0, 1.8, 0, 6.0, fx="Grapple", cue="Click", tele="Wind")
mv("vane_flick", "Vane Flick", "Bash", 8, 4, 12, 6, 20, 10, 2.2, 2.0, 60, 0.6, cue="Clank", tele="Coil")
mv("grapple_line", "Grapple Line", "Special", 8, 6, 12, 6, 8, 16, 14.0, 1.6, 0, 12.0, fx="Grapple", cue="Click", tele="Wind")
mv("fluke_swing", "Fluke Swing", "Light", 12, 6, 16, 26, 16, 14, 4.0, 2.8, 120, 1.0)
mv("fluke_slam", "Mooring Slam", "Heavy", 26, 6, 26, 56, 42, 32, 4.2, 3.6, 0, 1.5, cue="Clank")
mv("winch_snare", "Winch Snare", "Bash", 14, 8, 16, 8, 24, 16, 7.0, 1.6, 0, 0, fx="Tether", cue="Click", tele="Wind")
mv("grid_surge", "Grid Surge", "Burst", 24, 8, 24, 30, 36, 40, 0.0, 7.0, 0, 0, fx="Tether", cue="Charge", tele="Ring", avoid="Distance")
mv("null_touch", "Null Touch", "Light", 8, 5, 12, 16, 10, 10, 3.2, 2.0, 60, 0.8, fx="Mimic", cue="Click")
mv("null_strike", "Null Strike", "Heavy", 18, 6, 20, 36, 26, 22, 3.6, 2.4, 100, 1.8)
mv("void_palm", "Void Palm", "Bash", 10, 4, 14, 8, 26, 12, 2.4, 2.0, 0, 1.0, cue="Click", tele="Coil")
mv("echo_rack", "Echo Rack", "Special", 12, 6, 16, 18, 14, 20, 3.4, 2.4, 60, 2.0, fx="Mimic", cue="Bell", tele="Spread")
mv("censer_swing", "Censer Swing", "Light", 10, 6, 14, 20, 14, 12, 3.8, 2.6, 140, 0.8, cue="Bell")
mv("bell_toll", "Bell Toll", "Heavy", 24, 6, 24, 48, 40, 30, 0.6, 4.8, 0, 0, cue="Bell", tele="Ring", avoid="Distance")
mv("vigil_flare", "Vigil Flare", "Special", 16, 4, 18, 22, 12, 26, 2.0, 1.6, proj=(36, 1.6, 50, 0, 3, 10), cue="Charge")
mv("lamp_lance", "Lamp Lance", "Heavy", 20, 6, 20, 42, 28, 24, 5.0, 1.8, 0, 3.0, cue="Charge")
mv("beam_sweep", "Beam Sweep", "Light", 10, 8, 14, 16, 10, 12, 6.0, 1.6, 90, 0, fx="Burn", cue="Hiss", tele="Wind")
mv("searchlight", "Searchlight", "Special", 18, 20, 18, 4, 30, 24, 10.0, 3.0, 40, 0, fx="Static", cue="Drone", tele="Spread", avoid="Distance")
mv("breakwater_fist", "Breakwater Fist", "Light", 12, 6, 16, 26, 18, 14, 3.8, 2.8, 60, 1.4, cue="Clank")
mv("seawall_slam", "Seawall Slam", "Heavy", 26, 6, 26, 58, 44, 32, 1.0, 5.0, 0, 0, cue="Clank", tele="Ring", avoid="Distance")
mv("floodgate", "Floodgate", "Special", 22, 10, 22, 26, 30, 34, 4.0, 4.0, 90, 4.0, hyper=True, cue="Charge", tele="Coil")
mv("deacon_mace", "Deacon Mace", "Light", 14, 6, 18, 14, 10, 12, 3.0, 2.2, 90, 1.0)
mv("deacon_crush", "Deacon Crush", "Heavy", 22, 6, 24, 30, 24, 22, 3.2, 2.6, 0, 1.6, cue="Clank")
mv("deacon_shove", "Buckler Shove", "Bash", 14, 5, 16, 6, 22, 12, 2.2, 2.0, 0, 1.2, cue="Clank", tele="Coil")
mv("lancer_burst", "Lancer Burst", "Light", 16, 3, 16, 8, 3, 10, 2.0, 0.8, proj=(42, 0.8, 50, 0, 3, 6), cue="Click")
mv("lancer_pike", "Lancer Pike", "Heavy", 20, 5, 20, 28, 20, 22, 4.8, 1.6, 0, 3.0, cue="Charge")
mv("coax_mg", "Coaxial Chatter", "Light", 18, 3, 14, 6, 2, 8, 2.0, 0.6, proj=(48, 0.6, 50, 0, 4, 10), cue="Click", tele="Wind")
mv("tank_shell", "Tank Shell", "Heavy", 30, 3, 30, 22, 12, 20, 2.0, 1.0, proj=(40, 1.0, 70, 2, 1, 0), cue="Charge")
mv("smoke_screen", "Smoke Screen", "Special", 18, 4, 20, 0, 0, 20, 0.0, 6.0, 0, 0, fx="Static", cue="Hiss", tele="Ring")
mv("drone_burst", "Seraph Burst", "Light", 20, 3, 14, 6, 2, 8, 2.0, 0.6, proj=(45, 0.6, 50, 0, 3, 8), cue="Drone", tele="Spread")
mv("drone_dive", "Seraph Dive", "Heavy", 24, 6, 26, 18, 16, 20, 2.4, 2.0, 0, 10.0, cue="Shriek", tele="Rear")
mv("spotter_flare", "Spotter Flare", "Light", 20, 3, 16, 4, 2, 6, 2.0, 0.8, proj=(30, 0.8, 60, 0, 1, 0), cue="Click", tele="Wind")
mv("psalter_shell", "Psalter Shell", "Heavy", 48, 3, 40, 30, 20, 24, 2.0, 3.0, proj=(22, 3.0, 140, 14, 1, 0), cue="Bell", tele="Raise", avoid="Distance")
mv("flak_burst", "Flak Burst", "Light", 30, 3, 20, 10, 4, 10, 2.0, 1.4, proj=(50, 1.4, 50, 0, 3, 12), cue="Click", tele="Wind")
mv("flak_salvo", "Flak Salvo", "Heavy", 40, 3, 30, 16, 8, 20, 2.0, 1.8, proj=(46, 1.8, 60, 0, 5, 16), cue="Charge", tele="Raise")

# --- Kaiju player moves
mv("coil_lash", "Coil Lash", "Light", 10, 6, 14, 20, 12, 12, 4.0, 2.6, 160, 0.6, cue="Hiss", tele="Coil")
mv("abyss_bite", "Abyss Bite", "Heavy", 18, 5, 20, 40, 26, 22, 4.6, 2.2, 0, 2.5, cue="Roar", tele="Rear")
mv("gill_hook", "Gill Hook", "Bash", 10, 5, 14, 10, 22, 12, 2.6, 2.2, 60, 0.8, cue="Gurgle", tele="Coil")
mv("constrict", "Constrict", "Grab", 16, 8, 24, 36, 30, 30, 3.6, 2.4, 0, 2.0, fx="Constrict", cue="Gurgle", tele="Coil")
mv("wing_buffet", "Wing Buffet", "Light", 10, 8, 12, 14, 14, 12, 3.2, 3.0, 180, 0, cue="Drone", tele="Spread")
mv("moth_stab", "Proboscis Stab", "Heavy", 18, 5, 20, 36, 24, 22, 4.4, 1.8, 0, 3.0, cue="Shriek", tele="Rear")
mv("dust_swipe", "Dust Swipe", "Bash", 10, 5, 14, 8, 22, 12, 2.8, 2.4, 90, 0.4, fx="Spores", cue="Drone", tele="Coil")
mv("spore_bloom", "Spore Bloom", "Burst", 22, 20, 20, 14, 10, 32, 0.0, 7.0, 0, 0, fx="Spores", cue="Drone", tele="Spread", avoid="Distance")
mv("chorus_grasp", "Grasping Chorus", "Light", 8, 8, 12, 12, 8, 10, 3.4, 2.6, 90, 0.6, cue="Shriek", tele="Spread")
mv("hymn_crush", "Hymn of Weight", "Heavy", 20, 6, 20, 36, 26, 24, 3.4, 3.2, 0, 1.0, cue="Bell", tele="Rear")
mv("many_hands", "Many Hands", "Bash", 10, 6, 14, 8, 22, 12, 2.8, 2.6, 120, 0.4, cue="Shriek", tele="Coil")
mv("schism", "Schism", "Summon", 16, 4, 20, 0, 0, 30, 0.0, 3.0, 0, 0, fx="Split", cue="Shriek", tele="Spread")
mv("maw_snap", "Maw Snap", "Light", 8, 5, 14, 18, 10, 12, 3.8, 2.2, 40, 1.0, cue="Gurgle", tele="Coil")
mv("maw_erupt", "Erupt", "Burst", 24, 6, 24, 46, 34, 28, 1.0, 5.0, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("grave_crush", "Grave Crush", "Heavy", 20, 6, 22, 42, 30, 24, 3.6, 2.6, 0, 1.6, cue="Gurgle", tele="Rear")
mv("shovel_rake", "Shovel Rake", "Bash", 12, 5, 14, 10, 24, 12, 2.6, 2.4, 80, 0.6, cue="Gurgle", tele="Coil")
mv("undermine", "Undermine", "Special", 20, 8, 24, 38, 30, 34, 0.0, 5.0, 0, 0, fx="Burrow", cue="Roar", tele="Ring", avoid="Distance")
mv("keen_rake", "Keening Rake", "Light", 9, 6, 13, 18, 12, 11, 3.6, 2.4, 120, 0.8, cue="Shriek", tele="Coil")
mv("mourning_crush", "Mourning Crush", "Heavy", 20, 6, 22, 42, 28, 24, 3.8, 2.8, 0, 1.6, cue="Gurgle", tele="Rear")
mv("veil_sweep", "Veil Sweep", "Bash", 10, 6, 14, 8, 24, 12, 2.8, 2.6, 140, 0.4, cue="Hiss", tele="Spread")
mv("floodmourn", "Floodmourn", "Burst", 22, 10, 22, 20, 22, 32, 0.0, 6.0, 0, 0, fx="Flood", cue="Gurgle", tele="Ring", avoid="Distance")
mv("wrecker_lash", "Wrecker Lash", "Light", 10, 6, 14, 20, 12, 12, 4.4, 2.4, 120, 0.4, cue="Hiss", tele="Coil")
mv("hull_breaker", "Hull Breaker", "Heavy", 22, 6, 22, 46, 32, 26, 4.0, 3.0, 0, 2.4, cue="Roar", tele="Rear")
mv("siren_lure", "Siren Lure", "Special", 20, 18, 20, 6, 26, 26, 8.0, 3.0, 50, 0, fx="Static", cue="Shriek", tele="Spread", avoid="Distance")
mv("pincer_snip", "Pincer Snip", "Light", 9, 5, 13, 18, 12, 11, 3.2, 2.0, 30, 0.6, cue="Clank", tele="Coil")
mv("carapace_slam", "Carapace Slam", "Heavy", 24, 6, 24, 50, 36, 28, 1.2, 4.6, 0, 0, cue="Clank", tele="Ring", avoid="Distance")
mv("brood_call", "Brood Call", "Summon", 20, 4, 20, 0, 0, 30, 0.0, 3.0, 0, 0, fx="Split", cue="Gurgle", tele="Spread")
mv("undertow_lash", "Undertow Lash", "Light", 10, 6, 14, 22, 14, 12, 4.6, 2.6, 150, 0.6, cue="Hiss", tele="Coil")
mv("maelstrom", "Maelstrom", "Heavy", 24, 8, 24, 48, 34, 28, 0.6, 5.4, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("high_tide", "High Tide", "Special", 22, 10, 22, 22, 24, 34, 0.0, 6.4, 0, 0, fx="Flood", cue="Gurgle", tele="Ring", avoid="Distance")
mv("leech_bite", "Leech Bite", "Light", 12, 5, 16, 8, 6, 8, 2.0, 1.4, 30, 1.2, cue="Gurgle", tele="Coil")
mv("leech_latch", "Leech Latch", "Heavy", 18, 6, 22, 14, 14, 16, 2.4, 1.6, 0, 3.0, cue="Shriek", tele="Rear")
mv("spawn_nip", "Spawn Nip", "Light", 12, 4, 14, 6, 4, 6, 1.8, 1.2, 30, 1.0, cue="Gurgle", tele="Coil")
mv("spawn_pounce", "Spawn Pounce", "Heavy", 18, 5, 20, 12, 10, 14, 2.2, 1.4, 0, 5.0, cue="Shriek", tele="Coil")
mv("juv_lash", "Juvenile Lash", "Light", 12, 6, 16, 14, 10, 10, 3.4, 2.2, 140, 0.4, cue="Hiss", tele="Coil")
mv("juv_bite", "Juvenile Bite", "Heavy", 20, 5, 22, 28, 20, 18, 3.8, 2.0, 0, 2.0, cue="Roar", tele="Rear")
mv("juv_barb", "Barb Flick", "Bash", 12, 5, 16, 6, 18, 10, 2.4, 2.0, 60, 0.4, cue="Hiss", tele="Coil")
mv("brine_spit", "Brine Spit", "Light", 18, 3, 16, 10, 4, 10, 2.0, 1.0, proj=(30, 1.0, 60, 6, 1, 0), cue="Gurgle", tele="Rear")
mv("bile_lob", "Bile Lob", "Heavy", 28, 3, 26, 22, 12, 20, 2.0, 2.4, proj=(20, 2.4, 100, 14, 1, 0), fx="Burn", cue="Gurgle", tele="Rear", avoid="Distance")
mv("spitter_claw", "Spitter Claw", "Bash", 12, 5, 14, 6, 18, 10, 2.2, 1.8, 40, 0.4, cue="Hiss", tele="Coil")
mv("shell_pinch", "Shell Pinch", "Light", 12, 5, 16, 14, 10, 10, 2.8, 1.8, 30, 0.6, cue="Clank", tele="Coil")
mv("shell_ram", "Shell Ram", "Heavy", 22, 8, 24, 26, 30, 20, 2.4, 2.4, 0, 6.0, hyper=True, cue="Clank", tele="Coil")
mv("lurk_snap", "Lurk Snap", "Light", 10, 5, 16, 14, 10, 10, 3.0, 2.0, 40, 1.0, cue="Gurgle", tele="Coil")
mv("lurk_erupt", "Lurk Erupt", "Heavy", 22, 6, 24, 30, 26, 22, 0.8, 4.0, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("lurk_rake", "Lurk Rake", "Bash", 12, 5, 14, 6, 20, 10, 2.4, 2.0, 60, 0.4, cue="Gurgle", tele="Coil")
mv("devour", "Devour", "Eat", 10, 8, 20, 0, 0, 0, 3.0, 2.4, 0, 0.6, cue="Gurgle", tele="Rear")
mv("heretic_form", "HERETIC FORM", "Heretic", 30, 12, 40, 90, 80, 50, 0.0, 8.0, 0, 0, hyper=True, cue="Bell", tele="Spread", avoid="Distance",
   lore="Saint and monster stop pretending. For one breath, neither is in charge.")

# --- Boss moves (windup >= 18 ticks = 300 ms)
def boss_set(prefix, cue_l="Swing", cue_h="Clank", flavour=None):
    pass
mv("warden_combo", "Warden's Litany", "Light", 18, 6, 14, 30, 16, 0, 3.6, 2.4, 110, 1.6)
mv("warden_cleave", "Warden's Cleave", "Heavy", 24, 6, 24, 52, 34, 0, 4.4, 3.0, 150, 2.5)
mv("warden_bash", "Warden's Rebuke", "Bash", 20, 5, 18, 18, 40, 0, 2.6, 2.4, 0, 4.0, cue="Clank", tele="Coil")
mv("warden_charge", "Pilgrim Charge", "Special", 30, 10, 26, 40, 50, 0, 2.8, 2.8, 0, 14.0, hyper=True, cue="Charge", tele="Coil")
mv("warden_feint", "False Absolution", "Heavy", 22, 6, 24, 56, 36, 0, 4.2, 2.8, 140, 2.0, feint=16, lore="He holds the blade one breath longer than you expect. Every time.")
mv("cinder_spray", "Furnace Spray", "Light", 20, 12, 16, 18, 8, 0, 5.4, 2.8, 40, 0, fx="Burn", cue="Hiss", tele="Coil")
mv("cinder_gout", "Kiln Gout", "Heavy", 28, 14, 26, 38, 26, 0, 7.0, 3.2, 30, 0, fx="Burn", cue="Hiss")
mv("cinder_mortar", "Mortar Psalm", "Special", 32, 3, 30, 30, 12, 0, 2.0, 3.0, proj=(18, 3.0, 110, 14, 3, 18), fx="Burn", cue="Charge", avoid="Distance")
mv("cinder_vent", "Vent Bloom", "Burst", 26, 10, 30, 28, 30, 0, 0.0, 6.0, 0, 0, fx="Burn", cue="Hiss", tele="Ring", avoid="Distance")
mv("wren_jab", "Kestrel Jab", "Light", 18, 4, 12, 24, 12, 0, 4.0, 1.8, 20, 2.0, cue="Click")
mv("wren_reel", "Reel and Cut", "Heavy", 22, 6, 20, 40, 30, 0, 9.0, 1.8, 0, 8.0, fx="Grapple", cue="Click", tele="Wind")
mv("wren_dance", "Starling Dance", "Special", 26, 8, 22, 30, 20, 0, 3.2, 3.2, 360, 0, cue="Swing", tele="Spread", avoid="Distance")
mv("wren_feint", "Vane Feint", "Light", 18, 4, 16, 26, 14, 0, 3.8, 1.8, 40, 2.0, feint=12, cue="Click")
mv("anchor_swing", "Fluke Sweep", "Light", 20, 6, 18, 32, 20, 0, 4.4, 3.0, 140, 1.0)
mv("anchor_slam", "Drydock Slam", "Heavy", 30, 6, 30, 64, 46, 0, 4.4, 4.0, 0, 1.6, cue="Clank")
mv("anchor_surge", "Harbor Surge", "Burst", 28, 8, 28, 36, 40, 0, 0.0, 8.0, 0, 0, fx="Tether", cue="Charge", tele="Ring", avoid="Distance")
mv("anchor_snare", "Chain Snare", "Bash", 22, 8, 20, 14, 30, 0, 9.0, 1.8, 0, 0, fx="Tether", cue="Click", tele="Wind")
mv("vesper_censer", "Censer Arc", "Light", 20, 6, 16, 28, 18, 0, 4.2, 2.8, 150, 1.0, cue="Bell")
mv("vesper_toll", "Great Toll", "Heavy", 30, 6, 30, 60, 50, 0, 0.6, 6.0, 0, 0, cue="Bell", tele="Ring", avoid="Distance")
mv("vesper_flare", "Vigil Rain", "Special", 26, 3, 24, 22, 10, 0, 2.0, 2.0, proj=(30, 2.0, 80, 6, 5, 30), cue="Charge", avoid="Distance")
mv("vesper_rise", "The Sleeper Wakes", "Burst", 36, 10, 30, 40, 60, 0, 0.0, 9.0, 0, 0, hyper=True, cue="Bell", tele="Spread", avoid="Distance")
mv("lamp_beam", "Lamp Beam", "Light", 22, 10, 18, 20, 12, 0, 8.0, 2.0, 80, 0, fx="Burn", cue="Hiss", tele="Wind")
mv("lamp_spear", "Light Spear", "Heavy", 26, 6, 24, 48, 30, 0, 6.0, 2.0, 0, 6.0, cue="Charge")
mv("lamp_blind", "Blinding Sweep", "Special", 24, 20, 24, 6, 34, 0, 12.0, 3.2, 40, 0, fx="Static", cue="Drone", tele="Spread", avoid="Distance")
mv("lamp_salvo", "Signal Salvo", "Special", 28, 3, 24, 18, 8, 0, 2.0, 1.4, proj=(38, 1.4, 70, 0, 5, 20), cue="Click")
mv("colossus_fist", "Seawall Fist", "Light", 24, 6, 20, 40, 24, 0, 5.0, 3.6, 60, 2.0, cue="Clank")
mv("colossus_slam", "Breakwater Slam", "Heavy", 34, 6, 34, 70, 50, 0, 1.4, 7.0, 0, 0, cue="Clank", tele="Ring", avoid="Distance")
mv("colossus_flood", "Open the Floodgates", "Burst", 38, 12, 34, 44, 40, 0, 0.0, 10.0, 0, 0, cue="Charge", tele="Spread", avoid="Distance")
mv("colossus_sweep", "Tidewall Sweep", "Heavy", 30, 8, 28, 52, 36, 0, 6.0, 3.6, 170, 0, cue="Swing", tele="Wind")
mv("lev_lash", "Leviathan Lash", "Light", 20, 6, 18, 30, 18, 0, 5.4, 3.0, 170, 0.8, cue="Hiss", tele="Coil")
mv("lev_bite", "Leviathan Bite", "Heavy", 24, 5, 24, 54, 34, 0, 5.4, 2.6, 0, 4.0, cue="Roar", tele="Rear")
mv("lev_constrict", "The Coil Tightens", "Grab", 28, 8, 30, 50, 44, 0, 4.2, 3.0, 0, 3.0, fx="Constrict", cue="Gurgle", tele="Coil")
mv("lev_breach", "Breach", "Burst", 34, 8, 30, 46, 40, 0, 0.0, 7.0, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("lev_feint", "Undertow Feint", "Heavy", 22, 5, 24, 52, 32, 0, 5.2, 2.6, 0, 4.0, feint=18, cue="Roar", tele="Rear")
mv("moth_buffet", "Cathedral Buffet", "Light", 20, 8, 16, 22, 20, 0, 4.0, 4.0, 180, 0, cue="Drone", tele="Spread")
mv("moth_dive", "Lure Dive", "Heavy", 26, 6, 26, 44, 30, 0, 3.6, 2.6, 0, 14.0, cue="Shriek", tele="Rear")
mv("moth_bloom", "Spore Cathedral", "Burst", 28, 24, 24, 20, 14, 0, 0.0, 8.0, 0, 0, fx="Spores", cue="Drone", tele="Spread", avoid="Distance")
mv("moth_dust", "Dust Rain", "Special", 24, 3, 24, 16, 8, 0, 2.0, 2.6, proj=(16, 2.6, 100, 6, 6, 40), fx="Spores", cue="Drone", avoid="Distance")
mv("choir_grasp", "Chorus of Hands", "Light", 18, 8, 16, 22, 12, 0, 4.0, 3.4, 120, 1.0, cue="Shriek", tele="Spread")
mv("choir_crush", "Hymn of Weight", "Heavy", 24, 6, 24, 46, 32, 0, 3.6, 3.8, 0, 1.4, cue="Bell", tele="Rear")
mv("choir_split", "Schism", "Summon", 24, 4, 24, 0, 0, 0, 0.0, 3.0, 0, 0, fx="Split", cue="Shriek", tele="Spread")
mv("choir_recombine", "Recombine", "Burst", 30, 8, 26, 30, 30, 0, 0.0, 6.0, 0, 0, fx="Recombine", cue="Bell", tele="Ring", avoid="Distance")
mv("maw_ambush", "Grave Ambush", "Special", 30, 8, 30, 48, 36, 0, 0.0, 5.0, 0, 0, fx="Burrow", cue="Roar", tele="Ring", avoid="Distance")
mv("maw_crush", "Ossuary Crush", "Heavy", 24, 6, 24, 50, 34, 0, 4.4, 3.0, 0, 2.0, cue="Gurgle", tele="Rear")
mv("maw_rake", "Shovel Sweep", "Light", 18, 6, 18, 28, 18, 0, 3.8, 3.0, 130, 0.6, cue="Gurgle", tele="Coil")
mv("maw_quake", "Undercity Quake", "Burst", 32, 10, 30, 36, 40, 0, 0.0, 8.0, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("wreck_lash", "Wrecker Lash", "Light", 20, 6, 18, 30, 18, 0, 5.0, 3.0, 140, 0.4, cue="Hiss", tele="Coil")
mv("wreck_breaker", "Keel Breaker", "Heavy", 26, 6, 26, 54, 36, 0, 4.4, 3.2, 0, 3.0, cue="Roar", tele="Rear")
mv("wreck_lure", "False Light", "Special", 26, 20, 24, 8, 36, 0, 10.0, 3.0, 50, 0, fx="Static", cue="Shriek", tele="Spread", avoid="Distance")
mv("wreck_feint", "Drowned Feint", "Heavy", 22, 6, 24, 50, 32, 0, 4.4, 3.2, 0, 3.0, feint=14, cue="Roar", tele="Rear")
mv("matri_snip", "Matriarch Snip", "Light", 20, 5, 16, 30, 18, 0, 4.0, 2.6, 40, 1.0, cue="Clank", tele="Coil")
mv("matri_slam", "Carapace Fall", "Heavy", 30, 6, 30, 62, 46, 0, 1.4, 6.0, 0, 0, cue="Clank", tele="Ring", avoid="Distance")
mv("matri_brood", "Market Brood", "Summon", 26, 4, 26, 0, 0, 0, 0.0, 3.0, 0, 0, fx="Split", cue="Gurgle", tele="Spread")
mv("matri_ram", "Shell Avalanche", "Special", 28, 10, 30, 44, 44, 0, 3.0, 3.6, 0, 12.0, hyper=True, cue="Clank", tele="Coil")
mv("mother_lash", "Undertow", "Light", 22, 6, 20, 36, 22, 0, 6.0, 3.4, 160, 0.4, cue="Hiss", tele="Coil")
mv("mother_maelstrom", "Maelstrom", "Heavy", 32, 10, 30, 56, 40, 0, 0.6, 8.0, 0, 0, cue="Roar", tele="Ring", avoid="Distance")
mv("mother_tide", "The Tide Comes In", "Special", 36, 12, 34, 40, 36, 0, 0.0, 10.0, 0, 0, fx="Flood", cue="Gurgle", tele="Spread", avoid="Distance")
mv("mother_bite", "Mother's Mouth", "Heavy", 26, 6, 26, 60, 38, 0, 5.4, 3.4, 0, 4.0, cue="Roar", tele="Rear")
mv("widow_rake", "Widow's Rake", "Light", 18, 6, 16, 28, 18, 0, 4.2, 2.8, 130, 1.0, cue="Shriek", tele="Coil")
mv("widow_flood", "Mourning Flood", "Burst", 28, 10, 28, 30, 30, 0, 0.0, 7.0, 0, 0, fx="Flood", cue="Gurgle", tele="Ring", avoid="Distance")
mv("null_copy", "Null Echo", "Heavy", 24, 6, 24, 48, 30, 0, 4.0, 2.8, 120, 2.0, fx="Mimic", cue="Bell")
mv("null_grasp", "Hollow Grasp", "Light", 18, 5, 16, 26, 14, 0, 3.6, 2.2, 80, 1.6, cue="Click")

# ---------------------------------------------------------------- visual templates
def P(shape, at, size, col, tex="plate", rot=(0, 0, 0), glow=False):
    d = dict(shape=E(shape), at=v3(*at), size=v3(*size))
    if any(rot): d["rot"] = v3(*rot)
    d["tex"] = tex
    d["col"] = c3(*col)
    if glow: d["glow"] = True
    return d

def shade(c, k):
    return tuple(max(0, min(255, int(x * k))) for x in c)

def vis(rig, slot, c1, c2, g, style="balanced", weapon=None, k=1.0):
    """Return prims for a part. k scales the part's bulk."""
    S = lambda *a: tuple(x * k for x in a)
    pr = []
    if rig in ("Humanoid",):
        mt = "rust" if style in ("hollow",) else "plate"
        if slot == "Frame":
            w = 3.2 * (1.2 if style == "heavy" else 0.85 if style == "light" else 1.0)
            pr += [P("Box", (0, 2.1, 0), (w * k, 3.0 * k, 2.0 * k), c1, mt),
                   P("Box", (0, 0.2, 0), (w * 0.7 * k, 1.0, 1.4 * k), c2, "grate"),
                   P("Box", (0, 3.75, 0), (1.0, 0.5, 1.0), c2, "plate")]
            if style == "ornate": pr.append(P("Wedge", (0, 2.4, 1.05), (0.6, 1.8, 0.3), g, "gold"))
            if style == "heavy": pr.append(P("Box", (0, 2.6, 1.05), (w * 0.8, 1.0, 0.4), c2, "hazard"))
            if style == "light": pr.append(P("Wedge", (0, 3.0, 0.9), (2.4, 0.6, 0.4), c2, "plate", rot=(0, 0, 180)))
            if style == "hollow": pr.append(P("Box", (0, 2.1, 0), (w * 0.5, 2.0, 2.1), (20, 20, 24), "grate"))
            if style == "huge": pr += [P("Cylinder", (0, 2.4, 0), (4.4, 3.4, 2.8), c1, "concrete")]
        elif slot == "Head":
            pr += [P("Box", (0, 0.6, 0), (1.1, 1.2, 1.2), c1, mt),
                   P("Box", (0, 0.65, 0.62), (0.8, 0.18, 0.1), g, "glow", glow=True)]
            if style in ("balanced", "ornate"): pr.append(P("Cylinder", (0, 1.5, 0), (1.4, 0.12, 1.4), g, "gold", glow=True))
            if style == "light": pr.append(P("Spike", (0, 1.3, -0.2), (0.25, 0.9, 0.5), c2, "plate", rot=(-30, 0, 0)))
            if style == "heavy": pr.append(P("Box", (0, 0.4, 0.4), (1.3, 0.6, 0.6), c2, "grate"))
            if style == "lamp": pr.append(P("Cylinder", (0, 1.4, 0.2), (1.0, 0.8, 1.0), g, "glow", glow=True))
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Box", (0.15 * s, 0.25, 0), S(1.3, 0.9, 1.3), c1, mt),
                   P("Tube", (0, -1.3, 0), S(0.8, 2.0, 0.8), c2, "grate"),
                   P("Box", (0, -3.0, 0.15), S(0.95, 1.9, 0.95), c1, mt)]
            if weapon == "shield":
                pr.append(P("Box", (0.55 * s, -2.7, 0.6), (0.3, 3.0, 2.2), c1, "plate"))
                pr.append(P("Wedge", (0.72 * s, -2.4, 0.6), (0.1, 1.4, 0.5), g, "gold"))
            elif weapon == "buckler":
                pr.append(P("Cylinder", (0.55 * s, -3.0, 0.5), (1.6, 0.25, 1.6), c1, "plate", rot=(0, 0, 90)))
            elif weapon == "sword":
                pr.append(P("Box", (0, -3.9, 2.0), (0.2, 0.5, 3.8), (200, 205, 215), "steel"))
                pr.append(P("Box", (0, -3.9, 0.3), (0.9, 0.2, 0.3), g, "gold"))
            elif weapon == "flamer":
                pr.append(P("Cylinder", (0, -3.9, 1.3), (0.6, 2.4, 0.6), (60, 50, 45), "rust", rot=(90, 0, 0)))
                pr.append(P("Sphere", (0, -3.9, 2.6), (0.4, 0.4, 0.4), (255, 140, 40), "glow", glow=True))
            elif weapon == "hook":
                pr.append(P("Spike", (0, -4.2, 0.9), (0.3, 1.6, 0.3), (190, 190, 200), "steel", rot=(60, 0, 0)))
            elif weapon == "anchor":
                pr.append(P("Box", (0, -4.6, 0.6), (0.4, 2.4, 0.4), (80, 80, 88), "rust"))
                pr.append(P("Wedge", (0, -5.6, 0.6), (2.0, 0.8, 0.6), (80, 80, 88), "rust", rot=(0, 0, 180)))
            elif weapon == "winch":
                pr.append(P("Cylinder", (0, -3.4, 0.7), (1.0, 1.0, 1.0), (90, 90, 70), "rust", rot=(0, 0, 90)))
            elif weapon == "gun":
                pr.append(P("Cylinder", (0, -3.6, 1.4), (0.45, 2.6, 0.45), (40, 40, 44), "steel", rot=(90, 0, 0)))
            elif weapon == "censer":
                pr.append(P("Sphere", (0, -5.0, 0.9), (1.1, 1.1, 1.1), g, "gold", glow=True))
            elif weapon == "lance":
                pr.append(P("Cone", (0, -3.8, 2.4), (0.6, 4.0, 0.6), (220, 220, 190), "steel", rot=(90, 0, 0)))
            elif weapon == "fist":
                pr.append(P("Box", (0, -4.3, 0.3), (1.6, 1.4, 1.6), c1, "concrete"))
            elif weapon == "void":
                pr.append(P("Sphere", (0, -4.1, 0.4), (0.7, 0.7, 0.7), (10, 10, 14), "grate"))
        elif slot == "Legs":
            for s in (-1, 1):
                pr += [P("Box", (0.8 * s, -1.2, 0), S(1.0, 2.2, 1.1), c1, mt),
                       P("Box", (0.85 * s, -3.4, 0.1), S(1.0, 2.3, 1.2), c2, "grate"),
                       P("Box", (0.85 * s, -4.4, 0.3), S(1.1, 0.4, 1.8), c1, "plate")]
                if style == "heavy": pr.append(P("Box", (1.1 * s, -2.4, 0), (0.5, 1.2, 1.4), c2, "hazard"))
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (0.9, 0.9, 0.45), g, "glow", glow=True),
                   P("Cylinder", (0, 0, -0.1), (1.3, 0.3, 1.3), c2, "grate", rot=(90, 0, 0))]
        elif slot == "Back":
            pr += [P("Box", (0, 0, -0.3), S(2.0, 2.2, 1.0), c1, mt)]
            if weapon == "thrusters":
                for s in (-1, 1):
                    pr.append(P("Cylinder", (0.6 * s, -1.3, -0.6), (0.5, 0.8, 0.5), g, "glow", glow=True))
            elif weapon == "mortar":
                pr.append(P("Cylinder", (0.5, 1.4, -0.4), (0.7, 1.6, 0.7), (60, 50, 45), "rust", rot=(-20, 0, 0)))
            elif weapon == "reel":
                pr.append(P("Cylinder", (0, 0.6, -0.9), (1.4, 0.6, 1.4), (150, 150, 160), "steel", rot=(90, 0, 0)))
            elif weapon == "capacitor":
                for s in (-1, 1):
                    pr.append(P("Cylinder", (0.6 * s, 0.4, -0.6), (0.6, 2.4, 0.6), g, "glow", glow=True))
            elif weapon == "bells":
                pr.append(P("Cone", (0, 1.6, -0.6), (1.2, 1.4, 1.2), g, "gold"))
            elif weapon == "rack":
                pr.append(P("Box", (0, 0.2, -0.9), (2.4, 0.3, 0.3), (14, 14, 18), "grate"))
            elif weapon == "smoke":
                pr.append(P("Cylinder", (0, 0.8, -0.6), (0.5, 1.0, 0.5), (80, 80, 80), "steel"))
            elif weapon == "floodgate":
                pr.append(P("Box", (0, 0.4, -1.0), (3.2, 2.6, 0.5), (120, 120, 110), "concrete"))
    elif rig == "Serpent":
        sc = "scale" if style != "veil" else "hide"
        if slot == "Frame":
            pr += [P("Sphere", (0, 0, 0), S(2.6, 2.3, 3.0), c1, sc),
                   P("Sphere", (0, -0.5, 0.2), S(2.0, 1.4, 2.6), c2, "belly")]
        elif slot == "Head":
            pr += [P("Box", (0, 0.2, 1.4), S(2.2, 1.6, 3.0), c1, sc),
                   P("Wedge", (0, -0.5, 1.8), S(2.0, 0.8, 2.6), c2, "belly"),
                   P("Sphere", (-0.8, 0.7, 2.4), (0.45, 0.35, 0.35), g, "glow", glow=True),
                   P("Sphere", (0.8, 0.7, 2.4), (0.45, 0.35, 0.35), g, "glow", glow=True)]
            if style == "veil": pr.append(P("Cone", (0, 1.2, 0.6), (2.6, 1.6, 2.0), (200, 210, 220), "hide"))
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Tube", (0.4 * s, 0, 0), S(0.9, 2.6, 0.9), c1, sc, rot=(0, 0, -70 * s)),
                   P("Spike", (2.4 * s, -0.6, 0.4), S(0.5, 1.6, 0.5), c2, "bone", rot=(20, 0, -110 * s))]
        elif slot == "Legs":
            pr += [P("Wedge", (0, 0.2, -1.2), S(0.4, 2.6, 2.4), c2, "fin"),
                   P("Wedge", (-1.2, -0.4, 0), S(1.8, 0.3, 1.4), c2, "fin", rot=(0, 0, 20)),
                   P("Wedge", (1.2, -0.4, 0), S(1.8, 0.3, 1.4), c2, "fin", rot=(0, 0, -20))]
        elif slot == "Core":
            pr += [P("Sphere", (0, 0.2, 0), (1.2, 1.2, 1.2), g, "glow", glow=True)]
        elif slot == "Back":
            for i in range(3):
                pr.append(P("Spike", (0, 1.2, -0.8 + i * 0.8), S(0.5, 1.4 - i * 0.2, 0.6), c2, "bone"))
            if weapon == "bladder": pr.append(P("Sphere", (0, 1.0, -1.5), (1.6, 1.2, 1.6), g, "hide", glow=True))
    elif rig == "Moth":
        if slot == "Frame":
            pr += [P("Sphere", (0, 0, 0), S(2.4, 2.8, 2.4), c1, "fur"),
                   P("Sphere", (0, -0.8, -2.4), S(2.0, 2.0, 3.6), c2, "hide")]
        elif slot == "Head":
            pr += [P("Sphere", (0, 0, 0), (1.6, 1.4, 1.4), c1, "fur"),
                   P("Sphere", (-0.6, 0.2, 0.5), (0.8, 0.8, 0.8), g, "glow", glow=True),
                   P("Sphere", (0.6, 0.2, 0.5), (0.8, 0.8, 0.8), g, "glow", glow=True),
                   P("Spike", (-0.4, 1.2, 0.2), (0.15, 1.6, 0.15), c2, "bone", rot=(-20, 0, 20)),
                   P("Spike", (0.4, 1.2, 0.2), (0.15, 1.6, 0.15), c2, "bone", rot=(-20, 0, -20))]
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Tube", (0, -0.9, 0.2), S(0.4, 2.0, 0.4), c2, "chitin", rot=(20, 0, 10 * s)),
                   P("Spike", (0.2 * s, -2.5, 0.8), S(0.3, 1.4, 0.3), c1, "bone", rot=(50, 0, 0))]
        elif slot == "Legs":
            for s in (-1, 1):
                pr += [P("Box", (3.0 * s, 0.6, 0), S(5.6, 0.12, 3.6), c1, "wing", rot=(0, 0, 10 * s)),
                       P("Box", (2.2 * s, -0.6, -1.2), S(3.6, 0.1, 2.4), c2, "wing", rot=(0, 0, -6 * s)),
                       P("Sphere", (3.4 * s, 0.7, 0.4), (1.0, 0.14, 1.0), g, "glow", glow=True)]
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (1.0, 1.0, 1.0), g, "glow", glow=True)]
        elif slot == "Back":
            for i, s in enumerate((-1, 1)):
                pr.append(P("Sphere", (0.8 * s, 0.4, -0.4), S(1.2, 1.2, 1.2), g, "hide", glow=True))
    elif rig == "Swarm":
        if slot == "Frame":
            pr += [P("Tube", (0, 1.2, 0), (0.7, 2.4, 0.5), c1, "pale"),
                   P("Sphere", (0, 2.7, 0), (0.6, 0.7, 0.6), c2, "pale")]
        elif slot == "Head":
            pr += [P("Sphere", (0, 0.5, 0), (1.2, 1.4, 1.0), c1, "pale"),
                   P("Box", (0, 0.5, 0.48), (0.8, 0.12, 0.1), g, "glow", glow=True),
                   P("Box", (0, 0.2, 0.48), (0.5, 0.12, 0.1), g, "glow", glow=True)]
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Tube", (0.2 * s, -1.0, 0.3), S(0.3, 2.2, 0.3), c1, "pale", rot=(30, 0, 15 * s)),
                   P("Tube", (0.5 * s, -0.8, 0.0), S(0.25, 2.0, 0.25), c2, "pale", rot=(10, 0, 35 * s))]
        elif slot == "Legs":
            for s in (-1, 1):
                pr.append(P("Tube", (0.3 * s, -1.0, 0), (0.3, 2.0, 0.3), c1, "pale"))
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (0.6, 0.6, 0.6), g, "glow", glow=True)]
        elif slot == "Back":
            pr += [P("Spike", (0, 0.4, -0.3), (0.3, 1.6, 0.3), c2, "bone", rot=(-30, 0, 0)),
                   P("Spike", (0.4, 0.2, -0.3), (0.3, 1.2, 0.3), c2, "bone", rot=(-30, 0, -25)),
                   P("Spike", (-0.4, 0.2, -0.3), (0.3, 1.2, 0.3), c2, "bone", rot=(-30, 0, 25))]
    elif rig == "Burrower":
        if slot == "Frame":
            pr += [P("Sphere", (0, 0, 0), S(5.0, 3.4, 7.0), c1, "hide"),
                   P("Sphere", (0, -0.8, -3.0), S(3.6, 2.4, 4.0), c2, "chitin")]
        elif slot == "Head":
            pr += [P("Sphere", (0, 0, 0.8), (3.2, 2.6, 2.6), c1, "hide"),
                   P("Wedge", (0, -0.6, 1.8), (2.8, 1.0, 1.4), (220, 210, 190), "bone"),
                   P("Wedge", (0, 0.5, 1.8), (2.8, 1.0, 1.4), (220, 210, 190), "bone", rot=(0, 0, 180)),
                   P("Sphere", (0, 0, 1.2), (1.8, 1.0, 1.4), g, "glow", glow=True)]
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Tube", (0.4 * s, 0, 0), S(1.0, 2.4, 1.0), c2, "chitin", rot=(60, 0, -40 * s)),
                   P("Wedge", (1.6 * s, -1.2, 1.6), S(1.6, 0.4, 1.6), (210, 200, 180), "bone")]
        elif slot == "Legs":
            pr += [P("Box", (0, 0, 0), S(4.0, 0.8, 6.0), c2, "chitin"),
                   P("Spike", (-2.0, 0, 0), (0.6, 1.2, 0.6), c2, "bone", rot=(0, 0, 90)),
                   P("Spike", (2.0, 0, 0), (0.6, 1.2, 0.6), c2, "bone", rot=(0, 0, -90))]
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (1.4, 1.4, 1.4), g, "glow", glow=True)]
        elif slot == "Back":
            for i in range(4):
                pr.append(P("Spike", ((i % 2) * 1.2 - 0.6, 0.6, -1.4 + i * 0.9), S(0.6, 1.8, 0.6), c2, "bone"))
    elif rig == "Quadruped":
        if slot == "Frame":
            pr += [P("Sphere", (0, 0, 0), S(3.2, 2.6, 4.6), c1, "hide"),
                   P("Sphere", (0, -0.4, -1.6), S(2.6, 2.2, 2.8), c2, "chitin")]
            if style == "veil": pr.append(P("Cone", (0, 1.4, -0.6), (3.6, 2.2, 4.4), (210, 220, 228), "hide"))
            if style == "shell": pr.append(P("Sphere", (0, 0.8, 0), S(4.0, 2.0, 5.2), c2, "chitin"))
        elif slot == "Head":
            pr += [P("Box", (0, 0, 0.6), (1.6, 1.4, 1.8), c1, "hide"),
                   P("Sphere", (-0.5, 0.4, 1.4), (0.35, 0.35, 0.35), g, "glow", glow=True),
                   P("Sphere", (0.5, 0.4, 1.4), (0.35, 0.35, 0.35), g, "glow", glow=True)]
            if style == "veil": pr.append(P("Cone", (0, 1.0, 0.4), (2.2, 1.6, 1.6), (210, 220, 228), "hide"))
            if style == "shell": pr += [P("Tube", (-0.5, 0.8, 1.0), (0.2, 1.2, 0.2), c2, "chitin"), P("Tube", (0.5, 0.8, 1.0), (0.2, 1.2, 0.2), c2, "chitin")]
        elif slot in ("LeftArm", "RightArm"):
            s = -1 if slot == "LeftArm" else 1
            pr += [P("Tube", (0, -1.2, 0), S(0.8, 2.4, 0.8), c1, "hide"),
                   P("Spike", (0, -2.6, 0.6), S(0.4, 1.0, 0.4), c2, "bone", rot=(70, 0, 0))]
            if weapon == "pincer": pr.append(P("Wedge", (0.3 * s, -2.8, 1.0), S(1.2, 0.6, 1.8), c2, "chitin"))
            if weapon == "gland": pr.append(P("Sphere", (0, -2.2, 0.6), (0.9, 0.9, 0.9), g, "hide", glow=True))
        elif slot == "Legs":
            for s in (-1, 1):
                pr += [P("Tube", (1.2 * s, -1.2, 0), S(0.8, 2.4, 0.8), c1, "hide"),
                       P("Box", (1.2 * s, -2.5, 0.2), (0.9, 0.3, 1.2), c2, "chitin")]
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (0.9, 0.9, 0.9), g, "glow", glow=True)]
        elif slot == "Back":
            pr += [P("Spike", (0, 0.4, 0), S(0.6, 1.4, 0.6), c2, "bone"),
                   P("Spike", (0, 0.3, -1.0), S(0.5, 1.1, 0.5), c2, "bone")]
            if weapon == "sac": pr.append(P("Sphere", (0, 0.4, -0.6), (1.4, 1.0, 1.4), g, "hide", glow=True))
            if weapon == "barnacles": pr += [P("Sphere", (x, 0.3, z), (0.6, 0.5, 0.6), c2, "chitin") for x, z in ((-0.6, 0.4), (0.7, -0.2), (0, -0.8))]
    elif rig == "Vehicle":
        if slot == "Frame":
            pr += [P("Box", (0, 0, 0), (3.0, 1.2, 4.6), c1, "camo"),
                   P("Wedge", (0, 0.2, 2.4), (3.0, 0.8, 0.6), c2, "plate")]
        elif slot == "Head":
            pr += [P("Box", (0, 0, 0), (2.0, 0.9, 2.2), c1, "camo"),
                   P("Box", (0.5, 0.6, 0.2), (0.4, 0.4, 0.4), g, "glow", glow=True)]
        elif slot in ("LeftArm", "RightArm"):
            if weapon == "tube":
                pr += [P("Cylinder", (0, 0.8, 0.8), (0.8, 3.4, 0.8), (50, 54, 50), "steel", rot=(50, 0, 0))]
            elif slot == "RightArm":
                pr += [P("Cylinder", (0, 0, 1.6), (0.4, 3.0, 0.4), (50, 54, 50), "steel", rot=(90, 0, 0))]
            else:
                pr += [P("Cylinder", (0, 0, 0.8), (0.2, 1.4, 0.2), (40, 40, 40), "steel", rot=(90, 0, 0))]
        elif slot == "Legs":
            for s in (-1, 1):
                pr.append(P("Box", (1.7 * s, 0, 0), (0.8, 1.0, 4.8), (40, 40, 38), "tread"))
        elif slot == "Core":
            pr += [P("Box", (0, 0, 0), (1.6, 0.8, 1.0), c2, "grate"), P("Box", (0, 0.45, 0), (0.6, 0.1, 0.6), g, "glow", glow=True)]
        elif slot == "Back":
            pr += [P("Cylinder", (-0.6, 0, 0), (0.4, 0.8, 0.4), c2, "steel"), P("Cylinder", (0.6, 0, 0), (0.4, 0.8, 0.4), c2, "steel")]
    elif rig == "Drone":
        if slot == "Frame":
            pr += [P("Sphere", (0, 0, 0), (1.6, 0.8, 1.6), c1, "plate"), P("Cylinder", (0, -0.2, 0), (2.4, 0.2, 2.4), c2, "plate")]
        elif slot == "Head":
            pr += [P("Sphere", (0, 0, 0), (0.6, 0.6, 0.6), g, "glow", glow=True)]
        elif slot in ("LeftArm", "RightArm"):
            pr += [P("Cylinder", (0, 0, 0.6), (0.2, 1.2, 0.2), (40, 40, 44), "steel", rot=(90, 0, 0))]
            if slot == "RightArm": pr.append(P("Spike", (0, -0.3, 0.6), (0.3, 0.9, 0.3), c2, "steel", rot=(110, 0, 0)))
        elif slot == "Legs":
            for x, z in ((-1.4, -1.4), (1.4, -1.4), (-1.4, 1.4), (1.4, 1.4)):
                pr.append(P("Cylinder", (x, 0, z), (1.4, 0.08, 1.4), (60, 60, 64), "grate"))
        elif slot == "Core":
            pr += [P("Sphere", (0, 0, 0), (0.5, 0.5, 0.5), g, "glow", glow=True)]
        elif slot == "Back":
            pr += [P("Spike", (0, 0.4, 0), (0.1, 1.2, 0.1), c2, "steel")]
    elif rig == "Turret":
        if slot == "Frame":
            pr += [P("Cylinder", (0, 1.0, 0), (3.6, 2.0, 3.6), c1, "concrete"), P("Box", (0, 2.4, 0), (2.4, 0.8, 2.4), c2, "plate")]
        elif slot == "Head":
            pr += [P("Box", (0, 0.4, 0), (0.2, 0.8, 0.2), c2, "steel"), P("Box", (0, 0.9, 0), (1.6, 0.6, 0.1), g, "grate", glow=True)]
        elif slot in ("LeftArm", "RightArm"):
            pr += [P("Cylinder", (0, 0.4, 1.0), (0.35, 2.6, 0.35), (50, 50, 50), "steel", rot=(60, 0, 0)),
                   P("Cylinder", (0.4, 0.4, 1.0), (0.35, 2.6, 0.35), (50, 50, 50), "steel", rot=(60, 0, 0))]
        elif slot == "Legs":
            pr += [P("Box", (0, -0.1, 0), (4.4, 0.4, 4.4), (70, 70, 66), "concrete")]
        elif slot == "Core":
            pr += [P("Box", (0, 0, 0), (1.0, 0.6, 1.0), g, "glow", glow=True)]
        elif slot == "Back":
            pr += [P("Box", (0, 0, 0), (1.2, 1.0, 0.8), c2, "steel")]
    assert pr, (rig, slot)
    return pr

# ---------------------------------------------------------------- parts
PARTS = []
def part(id, name, slot, side, weight, rig, c1, c2, g, src, stats=None, moves=(), style="balanced", weapon=None,
         loco=None, stamina=None, heal=None, parry=False, lore="", memory=None, quirks=(), complexity=1, k=1.0, rigkind=None):
    d = dict(id=id, name=name, slot=E(slot), side=E(side), weight=F(weight))
    st = {}
    for kk, vv in (stats or {}).items():
        st[kk] = vv if isinstance(vv, bool) else F(vv)
    d["stats"] = st
    if moves: d["moves"] = list(moves)
    if quirks: d["quirks"] = list(quirks)
    d["source"] = src
    if memory: d["memory"] = {k2: F(v2) for k2, v2 in memory.items()}
    if loco:
        lk, sp, sw, deep, climb = loco
        d["locomotion"] = dict(kind=E(lk), speed=F(sp), swim=F(sw), deep_water=deep, climb=climb)
    if stamina: d["stamina"] = E(stamina)
    if heal: d["heal"] = heal
    if slot == "Frame": d["rig"] = E(rigkind or rig)
    if parry: d["parry"] = True
    d["visual"] = dict(prims=vis(rig, slot, c1, c2, g, style, weapon, k))
    d["complexity"] = complexity
    if lore: d["lore"] = lore
    PARTS.append(d)
    return id

def chassis_parts(cid, side, rig, c1, c2, g, defs):
    """defs: list of 7 tuples (suffix, name, weight, stats, moves, extra-kwargs)."""
    src = E("Chassis", cid)
    ids = []
    for slot, (suf, name, w, st, mvs, kw) in zip(["Frame", "LeftArm", "RightArm", "Legs", "Core", "Head", "Back"], defs):
        ids.append(part(f"{cid}_{suf}", name, slot, side, w, rig, c1, c2, g, src, st, mvs, **kw))
    return ids

def src_boss(b):
    return E("Boss", b)
def src_enemy(e):
    return E("Enemy", e)

REP = lambda n: E("RepairCharges", n)
REG = E("Regrowth")

# ---- Saint chassis
IRON = chassis_parts("iron_saint", "Saint", "Humanoid", (150, 150, 160), (90, 92, 100), (255, 210, 120), [
    ("hull", "Cathedral Hull", 32, dict(hp=640, armor=26, capacity=100, poise=90, size=1.0), [], dict(heal=REP(5), style="ornate", lore="Poured from the melted bells of the old cathedral. Something inside it still tolls.")),
    ("shield", "Votive Shield", 13, dict(hp=190, armor=30, guard=0.85, impact=1.0), ["shield_bash"], dict(weapon="shield", parry=True, lore="Every dent is a prayer someone made.")),
    ("blade", "Penitent Blade", 12, dict(hp=150, armor=14, power=1.1, impact=1.0), ["blade_slash", "blade_cleave"], dict(weapon="sword", lore="It does not forgive. It only finishes.")),
    ("legs", "Pilgrim Legs", 20, dict(hp=220, armor=18), [], dict(loco=("Biped", 8.5, 0.7, False, True))),
    ("reliquary", "Heat Reliquary", 10, dict(hp=150, stamina=110, regen=38), [], dict(stamina="Heat", lore="A furnace in the shape of a reliquary. Overheat it and it vents like a sigh.")),
    ("halo", "Halo Sensor", 5, dict(hp=100, lock_range=48, fog_vision=12), [], dict(style="ornate")),
    ("thrusters", "Bulwark Thrusters", 10, dict(hp=120, armor=10), ["bulwark_charge"], dict(weapon="thrusters")),
])
CINDER = chassis_parts("st_cinder", "Saint", "Humanoid", (120, 60, 40), (70, 62, 58), (255, 120, 40), [
    ("hull", "Kiln Hull", 36, dict(hp=700, armor=30, capacity=106, poise=110, size=1.08), [], dict(heal=REP(5), style="heavy")),
    ("bellows", "Bellows Arm", 16, dict(hp=200, armor=24, guard=0.65, power=1.0, impact=1.1), ["bellows_bash", "flame_spray"], dict(weapon="flamer", parry=False)),
    ("pyre", "Pyre Lance", 18, dict(hp=190, armor=22, power=1.2, impact=1.1), ["flame_spray", "flame_gout"], dict(weapon="flamer")),
    ("legs", "Foundry Legs", 24, dict(hp=260, armor=26), [], dict(style="heavy", loco=("Biped", 7.0, 0.6, False, False))),
    ("furnace", "Furnace Heart", 12, dict(hp=170, stamina=130, regen=34), [], dict(stamina="Heat")),
    ("visor", "Smokeglass Visor", 5, dict(hp=110, lock_range=40, fog_vision=20), [], dict(style="heavy")),
    ("mortar", "Napalm Mortar", 16, dict(hp=140, armor=12), ["napalm_mortar"], dict(weapon="mortar")),
])
WREN = chassis_parts("st_wren", "Saint", "Humanoid", (70, 110, 140), (40, 50, 60), (140, 230, 255), [
    ("frame", "Sparrow Frame", 24, dict(hp=520, armor=14, capacity=95, poise=60, size=0.92), [], dict(heal=REP(4), style="light")),
    ("buckler", "Buckler Vane", 8, dict(hp=130, armor=16, guard=0.6, impact=1.2), ["vane_flick"], dict(weapon="buckler", parry=True)),
    ("talon", "Hook Talon", 9, dict(hp=130, armor=10, power=1.0, impact=1.0), ["hook_jab", "reel_strike"], dict(weapon="hook", parry=True)),
    ("legs", "Kestrel Legs", 14, dict(hp=170, armor=10), [], dict(loco=("Biped", 11.0, 0.8, False, True), k=0.85)),
    ("cell", "Wren Cell", 6, dict(hp=110, stamina=100, regen=55), [], dict(stamina="Battery")),
    ("optic", "Starling Optic", 3, dict(hp=80, lock_range=60, reveal=True, fog_vision=18), [], dict(style="light")),
    ("reel", "Grapple Reel", 7, dict(hp=100, armor=8), ["grapple_line"], dict(weapon="reel")),
])
ANCHOR = chassis_parts("st_anchor", "Saint", "Humanoid", (60, 80, 70), (150, 140, 60), (120, 255, 200), [
    ("hull", "Bollard Hull", 40, dict(hp=780, armor=34, capacity=112, poise=130, size=1.15), [], dict(heal=REP(6), style="heavy")),
    ("winch", "Chain Winch", 18, dict(hp=220, armor=26, guard=0.75, impact=1.1), ["winch_snare"], dict(weapon="winch", parry=True)),
    ("fluke", "Anchor Fluke", 20, dict(hp=230, armor=24, power=1.25, impact=1.3), ["fluke_swing", "fluke_slam"], dict(weapon="anchor")),
    ("pylons", "Pier Pylons", 28, dict(hp=300, armor=30), [], dict(style="heavy", loco=("Biped", 6.5, 1.0, True, False))),
    ("coupling", "Grid Coupling", 12, dict(hp=170, stamina=120, regen=46), [], dict(stamina="Battery", lore="Plugged into Harrowmouth's grid. When the lights go out, so does she.")),
    ("lamp", "Harbor Lamp", 6, dict(hp=120, lock_range=44, fog_vision=26), [], dict(style="lamp")),
    ("capacitor", "Surge Capacitor", 14, dict(hp=150, armor=14), ["grid_surge"], dict(weapon="capacitor")),
])
NULL = chassis_parts("st_null", "Saint", "Humanoid", (30, 30, 36), (60, 58, 66), (230, 230, 255), [
    ("reliquary", "Hollow Reliquary", 22, dict(hp=560, armor=16, capacity=104, poise=80, size=1.0), [], dict(heal=REP(4), style="hollow", lore="There was never a pilot. There was never not a pilot.")),
    ("gauntlet", "Empty Gauntlet", 9, dict(hp=150, armor=14, guard=0.7, impact=1.1), ["void_palm"], dict(weapon="void", parry=True)),
    ("hand", "Null Hand", 9, dict(hp=150, armor=12, power=1.05, impact=1.0), ["null_touch", "null_strike"], dict(weapon="void")),
    ("legs", "Vacant Legs", 15, dict(hp=190, armor=12), [], dict(style="hollow", loco=("Biped", 9.5, 1.0, False, True))),
    ("engine", "Absence Engine", 8, dict(hp=130, stamina=120, regen=44), [], dict(stamina="Fuel")),
    ("mask", "Blank Mask", 4, dict(hp=90, lock_range=50, reveal=True, fog_vision=30), [], dict(style="hollow")),
    ("rack", "Echo Rack", 8, dict(hp=110, armor=8), ["echo_rack"], dict(weapon="rack")),
])

# ---- Kaiju chassis
LEV = chassis_parts("leviathan", "Kaiju", "Serpent", (40, 70, 90), (150, 170, 150), (120, 255, 230), [
    ("coil", "Abyssal Coil", 30, dict(hp=680, armor=18, capacity=100, poise=90, size=1.15), ["devour"], dict(heal=REG, lore="Eleven ribs per segment. Nobody has counted the segments.")),
    ("gill_hook", "Gill Hook", 11, dict(hp=170, armor=12, guard=0.6, impact=1.1), ["gill_hook"], dict(parry=True)),
    ("barb", "Coil Barb", 12, dict(hp=170, armor=12, power=1.05, impact=1.0), ["coil_lash"], {}),
    ("tail", "Serpent Tail", 18, dict(hp=240, armor=14), [], dict(loco=("Serpent", 9.0, 1.7, True, False))),
    ("brine_heart", "Brine Heart", 10, dict(hp=160, stamina=120, regen=40), [], dict(stamina="Breath")),
    ("maw", "Abyss Maw", 6, dict(hp=140, armor=10, lock_range=44, fog_vision=30, power=1.1), ["abyss_bite"], {}),
    ("bladder", "Tidal Bladder", 12, dict(hp=130, armor=8), ["constrict"], dict(weapon="bladder")),
])
MOTH = chassis_parts("goliath_moth", "Kaiju", "Moth", (120, 110, 90), (70, 60, 50), (255, 230, 120), [
    ("thorax", "Lantern Thorax", 24, dict(hp=560, armor=10, capacity=95, poise=70, size=1.1), ["devour"], dict(heal=REG)),
    ("dust_leg", "Dust Foreleg", 7, dict(hp=120, armor=6, guard=0.5, impact=1.0), ["dust_swipe"], dict(parry=True)),
    ("hook_leg", "Hooked Foreleg", 8, dict(hp=120, armor=6, power=1.0, impact=1.0), ["wing_buffet"], {}),
    ("wings", "Cathedral Wings", 12, dict(hp=180, armor=4), [], dict(loco=("Wings", 10.5, 0.4, True, True), lore="Each wing holds the pattern of a rose window. The moth does not know what a window is.")),
    ("pollen_heart", "Pollen Heart", 8, dict(hp=120, stamina=110, regen=46), [], dict(stamina="Hunger")),
    ("lamp_eyes", "Lamp-Eye Crown", 4, dict(hp=100, lock_range=56, reveal=True, fog_vision=40, power=1.05), ["moth_stab"], {}),
    ("spore_sacs", "Spore Bloom", 10, dict(hp=110, armor=4), ["spore_bloom"], {}),
])
CHOIR = chassis_parts("pale_choir", "Kaiju", "Swarm", (225, 220, 210), (180, 170, 170), (255, 80, 80), [
    ("bodies", "Choir of Bodies", 22, dict(hp=600, armor=6, capacity=90, poise=60, size=0.95), ["devour"], dict(heal=REG, lore="HP is a headcount. Every body sings a different note of the same hymn.")),
    ("hands_l", "Grasping Hands", 8, dict(hp=120, armor=4, guard=0.55, impact=1.1), ["many_hands"], dict(parry=True)),
    ("hands_r", "Reaching Hands", 8, dict(hp=120, armor=4, power=1.0, impact=1.0), ["chorus_grasp", "hymn_crush"], {}),
    ("procession", "Shuffling Procession", 12, dict(hp=170, armor=4), [], dict(loco=("Swarm", 9.0, 1.0, False, True))),
    ("pulse", "Shared Pulse", 8, dict(hp=120, stamina=110, regen=30), [], dict(stamina="Blood")),
    ("faces", "Many Faces", 5, dict(hp=90, lock_range=46, fog_vision=24), [], {}),
    ("schism", "Schism", 8, dict(hp=100, armor=2), ["schism"], {}),
])
MAW = chassis_parts("mawgrave", "Kaiju", "Burrower", (90, 80, 70), (60, 54, 48), (255, 160, 90), [
    ("hide", "Grave Hide", 34, dict(hp=760, armor=28, capacity=105, poise=120, size=1.1), ["devour"], dict(heal=REG)),
    ("shovel", "Shovel Talon", 16, dict(hp=210, armor=20, guard=0.7, impact=1.2), ["shovel_rake"], dict(parry=True)),
    ("ossuary", "Ossuary Claw", 18, dict(hp=210, armor=20, power=1.15, impact=1.1), ["maw_snap", "grave_crush"], {}),
    ("belly", "Tunnel Belly", 24, dict(hp=280, armor=22), [], dict(loco=("Burrow", 7.5, 0.6, False, False))),
    ("marrow", "Marrow Heart", 12, dict(hp=170, stamina=130, regen=36), [], dict(stamina="Hunger")),
    ("maw", "Mawgrave Maw", 8, dict(hp=170, armor=16, lock_range=36, power=1.15), ["maw_erupt"], {}),
    ("undermine", "Undermine", 14, dict(hp=140, armor=12), ["undermine"], {}),
])
WIDOW = chassis_parts("tide_widow", "Kaiju", "Quadruped", (60, 80, 100), (200, 210, 220), (150, 220, 255), [
    ("veil", "Widow's Veil", 26, dict(hp=620, armor=14, capacity=100, poise=80, size=1.05), ["devour"], dict(heal=REG, style="veil", lore="She mourns the sea that left her on the shore. Where she walks, it returns.")),
    ("mourning", "Mourning Claw", 10, dict(hp=160, armor=10, guard=0.65, impact=1.15), ["veil_sweep"], dict(parry=True)),
    ("keening", "Keening Claw", 11, dict(hp=160, armor=10, power=1.1, impact=1.0), ["keen_rake", "mourning_crush"], {}),
    ("gait", "Drowned Gait", 16, dict(hp=220, armor=10), [], dict(loco=("Fins", 9.0, 1.5, True, True))),
    ("undertow", "Undertow Heart", 10, dict(hp=150, stamina=120, regen=42), [], dict(stamina="Breath")),
    ("crown", "Veiled Crown", 5, dict(hp=110, lock_range=50, reveal=True, fog_vision=34), [], dict(style="veil")),
    ("floodmourn", "Floodmourn", 10, dict(hp=120, armor=6), ["floodmourn"], {}),
])

# ---- Unique boss parts (Saint)
def boss_parts(bid, side, rig, c1, c2, g, defs):
    src = src_boss(bid)
    ids = []
    for slot, (suf, name, w, st, mvs, kw) in zip(["Frame", "LeftArm", "RightArm", "Legs", "Core", "Head", "Back"], defs):
        kw = dict(kw)
        kw.setdefault("complexity", 2)
        ids.append(part(f"{bid}_{suf}", name, slot, side, w, rig, c1, c2, g, src, st, mvs, **kw))
    return ids

VESPER = boss_parts("st_vesper", "Saint", "Humanoid", (210, 200, 170), (120, 100, 70), (255, 230, 160), [
    ("nave", "Nave Hull", 38, dict(hp=740, armor=30, capacity=110, poise=120, size=1.1), [], dict(heal=REP(6), style="ornate", lore="The Cathedral's sleeping Saint. Its pilot was buried in it; nobody checked if they were dead.")),
    ("chalice", "Chalice Guard", 14, dict(hp=200, armor=28, guard=0.8, impact=1.1), ["shield_bash"], dict(weapon="shield", parry=True)),
    ("censer", "Censer Arm", 15, dict(hp=190, armor=20, power=1.15, impact=1.2), ["censer_swing", "bell_toll"], dict(weapon="censer")),
    ("legs", "Vigil Legs", 22, dict(hp=240, armor=22), [], dict(loco=("Biped", 7.8, 0.7, False, False))),
    ("thurible", "Thurible Core", 12, dict(hp=160, stamina=120, regen=38), [], dict(stamina="Fuel")),
    ("mitre", "Mitre Sensor", 6, dict(hp=120, lock_range=52, reveal=True, fog_vision=16), [], dict(style="ornate")),
    ("bells", "Bell Rack", 16, dict(hp=140, armor=12), ["vigil_flare"], dict(weapon="bells")),
])
LAMPW = boss_parts("lampwright", "Saint", "Humanoid", (200, 190, 120), (60, 60, 70), (255, 255, 200), [
    ("tower", "Lighthouse Hull", 40, dict(hp=760, armor=26, capacity=110, poise=110, size=1.25), [], dict(heal=REP(5), style="heavy")),
    ("lantern", "Storm Lantern", 13, dict(hp=180, armor=18, guard=0.6, impact=1.0), ["beam_sweep"], dict(weapon="lance", parry=True)),
    ("spear", "Light Spear", 14, dict(hp=180, armor=16, power=1.15, impact=1.1), ["beam_sweep", "lamp_lance"], dict(weapon="lance")),
    ("legs", "Rock Legs", 26, dict(hp=280, armor=28), [], dict(style="heavy", loco=("Biped", 6.8, 0.8, True, False))),
    ("dynamo", "Fresnel Dynamo", 12, dict(hp=160, stamina=120, regen=40), [], dict(stamina="Battery")),
    ("lamp", "Great Lamp", 8, dict(hp=130, lock_range=70, reveal=True, fog_vision=50), [], dict(style="lamp")),
    ("beacon", "Beacon Array", 12, dict(hp=130, armor=8), ["searchlight"], dict(weapon="capacitor")),
])
COLOSSUS = boss_parts("breakwater_colossus", "Saint", "Humanoid", (130, 130, 120), (90, 90, 84), (120, 200, 255), [
    ("wall", "Seawall Hull", 48, dict(hp=900, armor=40, capacity=125, poise=160, size=1.35), [], dict(heal=REP(6), style="huge", lore="They poured the breakwater around something already standing in the surf.")),
    ("bulwark", "Tidewall Arm", 22, dict(hp=260, armor=34, guard=0.9, impact=1.2), ["shield_bash"], dict(weapon="fist", parry=True)),
    ("fist", "Breakwater Fist", 22, dict(hp=260, armor=30, power=1.3, impact=1.3), ["breakwater_fist", "seawall_slam"], dict(weapon="fist")),
    ("legs", "Caisson Legs", 32, dict(hp=340, armor=34), [], dict(style="heavy", loco=("Biped", 6.0, 1.0, True, False))),
    ("pump", "Pumphouse Core", 14, dict(hp=190, stamina=140, regen=36), [], dict(stamina="Fuel")),
    ("tower", "Signal Tower", 8, dict(hp=140, lock_range=56, fog_vision=30), [], dict(style="heavy")),
    ("floodgate", "Floodgate", 20, dict(hp=170, armor=20), ["floodgate"], dict(weapon="floodgate")),
])
# ---- Unique boss parts (Kaiju)
WRECKER = boss_parts("wrecker", "Kaiju", "Serpent", (30, 40, 50), (90, 110, 100), (255, 240, 160), [
    ("keel", "Keel-Spine", 32, dict(hp=700, armor=20, capacity=104, poise=100, size=1.2), ["devour"], dict(heal=REG, lore="Its spine is a ship's keel. Its lure is a ship's lamp. It is very patient.")),
    ("hook", "Wrecker Hook", 12, dict(hp=180, armor=14, guard=0.65, impact=1.1), ["gill_hook"], dict(parry=True)),
    ("lash", "Rigging Lash", 13, dict(hp=180, armor=14, power=1.1, impact=1.0), ["wrecker_lash", "hull_breaker"], {}),
    ("tail", "Drowned Tail", 20, dict(hp=250, armor=14), [], dict(loco=("Serpent", 9.5, 1.8, True, False))),
    ("heart", "Lantern Heart", 10, dict(hp=160, stamina=120, regen=42), [], dict(stamina="Blood")),
    ("lure", "False Light", 6, dict(hp=120, lock_range=60, reveal=True, fog_vision=44), [], {}),
    ("siren", "Siren Bladder", 12, dict(hp=130, armor=8), ["siren_lure"], dict(weapon="bladder")),
])
MATRI = boss_parts("brine_matriarch", "Kaiju", "Quadruped", (130, 70, 50), (200, 160, 110), (255, 120, 90), [
    ("carapace", "Market Carapace", 42, dict(hp=840, armor=38, capacity=118, poise=150, size=1.3), ["devour"], dict(heal=REG, style="shell", lore="The market stalls are still on her back. Some are still open.")),
    ("pincer_l", "Weighing Pincer", 18, dict(hp=240, armor=30, guard=0.85, impact=1.2), ["pincer_snip"], dict(weapon="pincer", parry=True)),
    ("pincer_r", "Gutting Pincer", 20, dict(hp=240, armor=28, power=1.25, impact=1.2), ["pincer_snip", "carapace_slam"], dict(weapon="pincer")),
    ("legs", "Stilt Legs", 26, dict(hp=300, armor=26), [], dict(loco=("Biped", 7.0, 1.2, True, True))),
    ("heart", "Brood Heart", 14, dict(hp=190, stamina=130, regen=34), [], dict(stamina="Hunger")),
    ("stalks", "Eye Stalks", 6, dict(hp=130, lock_range=46, reveal=True, fog_vision=28), [], dict(style="shell")),
    ("brood", "Brood Sacs", 16, dict(hp=150, armor=10), ["brood_call"], dict(weapon="sac")),
])
MOTHER = boss_parts("tide_mother", "Kaiju", "Serpent", (20, 50, 70), (230, 230, 220), (120, 255, 255), [
    ("mantle", "Tide Mantle", 46, dict(hp=920, armor=26, capacity=124, poise=150, size=1.45), ["devour"], dict(heal=REG, style="veil", lore="Every kaiju is one of her teeth. Every Saint is one of her bones.")),
    ("arm_l", "Mother's Left", 18, dict(hp=240, armor=18, guard=0.7, impact=1.2), ["gill_hook"], dict(parry=True)),
    ("arm_r", "Mother's Right", 20, dict(hp=240, armor=18, power=1.3, impact=1.2), ["undertow_lash", "maelstrom"], {}),
    ("tail", "Tide Tail", 26, dict(hp=300, armor=18), [], dict(loco=("Serpent", 8.5, 2.0, True, False))),
    ("heart", "Moon Heart", 14, dict(hp=200, stamina=140, regen=44), [], dict(stamina="Breath")),
    ("crown", "Pearl Crown", 8, dict(hp=150, lock_range=60, reveal=True, fog_vision=60, power=1.1), ["abyss_bite"], dict(style="veil")),
    ("tide", "High Tide", 18, dict(hp=170, armor=10), ["high_tide"], dict(weapon="bladder")),
])

# ---- Enemy parts (Saint side)
DEACON = [
    part("deacon_frame", "Deacon Frame", "Frame", "Saint", 20, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=460, armor=14, capacity=80, poise=60, size=0.8), style="light", heal=REP(3)),
    part("deacon_buckler", "Deacon Buckler", "LeftArm", "Saint", 8, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=110, armor=14, guard=0.6, impact=1.0), ["deacon_shove"], weapon="buckler", parry=True),
    part("deacon_mace", "Deacon Mace", "RightArm", "Saint", 10, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=110, armor=10, power=1.0, impact=1.1), ["deacon_mace", "deacon_crush"], weapon="censer"),
    part("deacon_legs", "Deacon Legs", "Legs", "Saint", 13, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=150, armor=10), loco=("Biped", 8.0, 0.7, False, True), k=0.9),
    part("deacon_cell", "Deacon Cell", "Core", "Saint", 7, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=100, stamina=100, regen=36), stamina="Fuel"),
    part("deacon_lamp", "Deacon Lamp", "Head", "Saint", 4, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=70, lock_range=38, fog_vision=10), style="lamp"),
    part("deacon_pack", "Deacon Pack", "Back", "Saint", 6, "Humanoid", (110, 110, 100), (70, 70, 64), (255, 200, 120), src_enemy("deacon"), dict(hp=80, armor=6), weapon="smoke"),
]
LANCER = [
    part("lancer_gun", "Lancer Gun-Arm", "LeftArm", "Saint", 9, "Humanoid", (90, 100, 110), (60, 64, 70), (255, 80, 60), src_enemy("lancer"), dict(hp=110, armor=10, guard=0.4, power=1.0, impact=0.9), ["lancer_burst"], weapon="gun"),
    part("lancer_pike", "Lancer Pike", "RightArm", "Saint", 11, "Humanoid", (90, 100, 110), (60, 64, 70), (255, 80, 60), src_enemy("lancer"), dict(hp=120, armor=10, power=1.1, impact=1.0), ["lancer_pike", "lancer_burst"], weapon="lance"),
    part("lancer_scope", "Lancer Scope", "Head", "Saint", 4, "Humanoid", (90, 100, 110), (60, 64, 70), (255, 80, 60), src_enemy("lancer"), dict(hp=70, lock_range=64, fog_vision=20), style="light"),
]
TANK = [
    part("tank_hull", "Militia Hull", "Frame", "Saint", 26, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=420, armor=30, capacity=90, poise=90, size=0.4), heal=REP(2)),
    part("coax_mg", "Coaxial MG", "LeftArm", "Saint", 6, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=90, armor=10, guard=0.3, impact=0.8), ["coax_mg"]),
    part("tank_cannon", "Militia Cannon", "RightArm", "Saint", 14, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=120, armor=16, power=1.15, impact=1.0), ["tank_shell", "coax_mg"]),
    part("tank_treads", "Militia Treads", "Legs", "Saint", 22, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=200, armor=24), loco=("Treads", 6.0, 0.3, False, True)),
    part("diesel_pack", "Diesel Pack", "Core", "Saint", 10, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=110, stamina=110, regen=30), stamina="Fuel"),
    part("periscope", "Periscope", "Head", "Saint", 3, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=60, lock_range=58, fog_vision=8)),
    part("smoke_rack", "Smoke Rack", "Back", "Saint", 6, "Vehicle", (90, 100, 70), (60, 66, 50), (255, 220, 120), src_enemy("tank"), dict(hp=70, armor=6), ["smoke_screen"]),
]
ARTY = [
    part("spotter_arm", "Spotter Arm", "LeftArm", "Saint", 6, "Vehicle", (100, 90, 70), (60, 56, 44), (255, 220, 120), src_enemy("artillery"), dict(hp=90, armor=8, guard=0.3, impact=0.9), ["spotter_flare"]),
    part("psalter_tube", "Psalter Tube", "RightArm", "Saint", 18, "Vehicle", (100, 90, 70), (60, 56, 44), (255, 220, 120), src_enemy("artillery"), dict(hp=130, armor=14, power=1.2, impact=1.1), ["psalter_shell", "spotter_flare"], weapon="tube"),
]
DRONE = [
    part("drone_shell", "Seraph Shell", "Frame", "Saint", 12, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=240, armor=8, capacity=70, poise=40, size=0.3), heal=REP(1)),
    part("drone_gun", "Seraph Gun", "LeftArm", "Saint", 4, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=60, armor=4, guard=0.2, impact=0.8), ["drone_burst"]),
    part("drone_prong", "Seraph Prong", "RightArm", "Saint", 5, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=60, armor=6, power=1.0, impact=1.0), ["drone_dive", "drone_burst"]),
    part("drone_rotors", "Seraph Rotors", "Legs", "Saint", 6, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=80, armor=2), loco=("Hover", 12.0, 1.0, True, True)),
    part("drone_cell", "Seraph Cell", "Core", "Saint", 3, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=60, stamina=90, regen=50), stamina="Battery"),
    part("drone_eye", "Seraph Eye", "Head", "Saint", 2, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=40, lock_range=66, reveal=True, fog_vision=30)),
    part("drone_antenna", "Seraph Antenna", "Back", "Saint", 2, "Drone", (200, 200, 210), (120, 120, 130), (120, 200, 255), src_enemy("drone"), dict(hp=40, armor=2)),
]
AA = [
    part("aa_mount", "AA Mount", "Frame", "Saint", 30, "Turret", (110, 110, 100), (80, 84, 70), (255, 80, 60), src_enemy("aa_battery"), dict(hp=480, armor=34, capacity=90, poise=200, size=0.5), heal=REP(1)),
    part("flak_l", "Flak Barrel L", "LeftArm", "Saint", 10, "Turret", (110, 110, 100), (80, 84, 70), (255, 80, 60), src_enemy("aa_battery"), dict(hp=120, armor=14, guard=0.2, impact=0.9), ["flak_burst"]),
    part("flak_r", "Flak Barrel R", "RightArm", "Saint", 10, "Turret", (110, 110, 100), (80, 84, 70), (255, 80, 60), src_enemy("aa_battery"), dict(hp=120, armor=14, power=1.1, impact=1.0), ["flak_salvo", "flak_burst"]),
    part("aa_base", "AA Plinth", "Legs", "Saint", 30, "Turret", (110, 110, 100), (80, 84, 70), (255, 80, 60), src_enemy("aa_battery"), dict(hp=260, armor=30), loco=("Treads", 0.6, 0.2, False, False)),
    part("aa_radar", "AA Radar", "Head", "Saint", 4, "Turret", (110, 110, 100), (80, 84, 70), (255, 80, 60), src_enemy("aa_battery"), dict(hp=70, lock_range=80, reveal=True, fog_vision=40)),
]
# ---- Enemy parts (Kaiju side)
def kp(id, name, slot, w, rig, c1, c2, g, eid, st, mvs=(), **kw):
    return part(id, name, slot, "Kaiju", w, rig, c1, c2, g, src_enemy(eid), st, mvs, **kw)
LEECH = [
    kp("leech_body", "Hull Leech", "Frame", 10, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=220, armor=4, capacity=70, poise=30, size=0.3), ["devour"], heal=REG),
    kp("leech_feeler", "Leech Feeler", "LeftArm", 3, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=50, guard=0.3, impact=1.0), ["leech_bite"]),
    kp("leech_proboscis", "Leech Proboscis", "RightArm", 3, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=50, power=1.0, impact=1.0), ["leech_bite", "leech_latch"]),
    kp("leech_legs", "Suckered Legs", "Legs", 5, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=70), loco=("Biped", 10.0, 1.2, True, True)),
    kp("leech_heart", "Leech Heart", "Core", 3, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=50, stamina=80, regen=30), stamina="Blood"),
    kp("leech_eyes", "Leech Eyes", "Head", 2, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=40, lock_range=30, fog_vision=20)),
    kp("leech_sac", "Leech Sac", "Back", 2, "Quadruped", (90, 60, 70), (60, 40, 46), (255, 90, 120), "hull_leech", dict(hp=40), weapon="sac"),
]
SPAWN = [
    kp("spawn_body", "Brood Spawn", "Frame", 8, "Quadruped", (60, 90, 80), (40, 60, 50), (200, 255, 160), "brood_spawn", dict(hp=160, armor=2, capacity=65, poise=20, size=0.25), ["devour"], heal=REG),
    kp("spawn_claw_l", "Spawn Claw", "LeftArm", 2, "Quadruped", (60, 90, 80), (40, 60, 50), (200, 255, 160), "brood_spawn", dict(hp=40, guard=0.2, impact=1.0), ["spawn_nip"]),
    kp("spawn_claw_r", "Spawn Talon", "RightArm", 2, "Quadruped", (60, 90, 80), (40, 60, 50), (200, 255, 160), "brood_spawn", dict(hp=40, power=0.9, impact=1.0), ["spawn_nip", "spawn_pounce"]),
    kp("spawn_heart", "Spawn Heart", "Core", 2, "Quadruped", (60, 90, 80), (40, 60, 50), (200, 255, 160), "brood_spawn", dict(hp=40, stamina=80, regen=34), stamina="Hunger"),
]
JUV = [
    kp("juv_coil", "Juvenile Coil", "Frame", 18, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=380, armor=10, capacity=85, poise=60, size=0.6), ["devour"], heal=REG),
    kp("juv_hook", "Juvenile Hook", "LeftArm", 6, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=90, armor=6, guard=0.5, impact=1.0), ["juv_barb"], parry=True),
    kp("juv_barb", "Juvenile Barb", "RightArm", 6, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=90, armor=6, power=1.0, impact=1.0), ["juv_lash"]),
    kp("juv_tail", "Juvenile Tail", "Legs", 10, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=130, armor=6), loco=("Serpent", 9.5, 1.6, True, False)),
    kp("juv_heart", "Juvenile Heart", "Core", 5, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=90, stamina=100, regen=40), stamina="Breath"),
    kp("juv_maw", "Juvenile Maw", "Head", 4, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=80, armor=4, lock_range=36, fog_vision=24), ["juv_bite"]),
    kp("juv_dorsal", "Juvenile Dorsal", "Back", 5, "Serpent", (50, 90, 100), (130, 160, 140), (140, 255, 220), "juvenile", dict(hp=70, armor=4)),
]
SPIT = [
    kp("spitter_body", "Spitter Body", "Frame", 14, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=300, armor=8, capacity=80, poise=40, size=0.45), ["devour"], heal=REG),
    kp("spitter_claw", "Spitter Claw", "LeftArm", 4, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=70, armor=4, guard=0.3, impact=1.0), ["spitter_claw"]),
    kp("spitter_gland", "Brine Gland", "RightArm", 5, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=70, armor=4, power=1.05, impact=0.9), ["brine_spit"], weapon="gland"),
    kp("spitter_legs", "Spitter Legs", "Legs", 8, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=100, armor=4), loco=("Biped", 8.0, 1.2, True, True)),
    kp("spitter_heart", "Bile Heart", "Core", 4, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=70, stamina=100, regen=36), stamina="Hunger"),
    kp("spitter_crest", "Bile Crest", "Head", 4, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=70, armor=4, lock_range=52, fog_vision=24), ["bile_lob"]),
    kp("spitter_bladder", "Bile Bladder", "Back", 4, "Quadruped", (100, 110, 60), (70, 80, 40), (200, 255, 80), "brine_spitter", dict(hp=60, armor=2), weapon="sac"),
]
SHELL = [
    kp("shell_body", "Shellback Carapace", "Frame", 26, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=520, armor=34, capacity=95, poise=110, size=0.65), ["devour"], heal=REG, style="shell"),
    kp("shell_pincer_l", "Shellback Pincer", "LeftArm", 10, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=150, armor=24, guard=0.8, impact=1.1), ["shell_pinch"], weapon="pincer", parry=True),
    kp("shell_pincer_r", "Shellback Crusher", "RightArm", 12, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=150, armor=22, power=1.1, impact=1.2), ["shell_pinch", "shell_ram"], weapon="pincer"),
    kp("shell_legs", "Shellback Legs", "Legs", 16, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=200, armor=20), loco=("Biped", 6.5, 1.0, True, True)),
    kp("shell_heart", "Shellback Heart", "Core", 8, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=120, stamina=110, regen=30), stamina="Blood"),
    kp("shell_eyestalks", "Eyestalks", "Head", 4, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=70, lock_range=40, fog_vision=20), style="shell"),
    kp("shell_barnacles", "Barnacle Crust", "Back", 10, "Quadruped", (120, 80, 60), (180, 150, 110), (255, 150, 90), "shellback", dict(hp=120, armor=20), weapon="barnacles", quirks=["barnacled"]),
]
LURK = [
    kp("lurk_body", "Gravelurk Hide", "Frame", 20, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=360, armor=16, capacity=85, poise=70, size=0.5), ["devour"], heal=REG),
    kp("lurk_talon", "Lurk Talon", "LeftArm", 7, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=90, armor=10, guard=0.5, impact=1.1), ["lurk_rake"], parry=True),
    kp("lurk_talon_r", "Lurk Hook", "RightArm", 7, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=90, armor=10, power=1.0, impact=1.0), ["lurk_snap"]),
    kp("lurk_belly", "Lurk Belly", "Legs", 10, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=130, armor=10), loco=("Burrow", 8.0, 0.5, False, False)),
    kp("lurk_heart", "Lurk Heart", "Core", 5, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=80, stamina=100, regen=34), stamina="Hunger"),
    kp("lurk_maw", "Lurk Maw", "Head", 5, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=90, armor=8, lock_range=34), ["lurk_erupt"]),
    kp("lurk_spines", "Lurk Spines", "Back", 5, "Burrower", (80, 74, 64), (56, 50, 44), (255, 170, 100), "gravelurk", dict(hp=70, armor=8)),
]

for p in PARTS:
    with open(os.path.join(OUT, "parts", p["id"] + ".ron"), "w") as f:
        f.write(f"// Part definition. Slot {p['slot'].name}, side {p['side'].name}.\n")
        f.write(ron(p) + "\n")

write("moves.ron", MOVES, "All moves. Timings are 60 Hz ticks: windup (+feint hold) -> active -> recovery.\nBoss moves must have windup >= tuning.min_boss_windup (300 ms). Validation enforces avoidability.")

# ---------------------------------------------------------------- quirks
Q = []
def q(id, name, desc, pol, effects, side=None, import_only=False, special=False):
    d = dict(id=id, name=name, desc=desc, polarity=E(pol), effects=effects)
    if side: d["side"] = E(side)
    if import_only: d["import_only"] = True
    if special: d["special"] = True
    Q.append(d)
SM = lambda s, m: E("StatMul", E(s), F(m))
q("twitching", "Twitching", "Attacks sometimes fire early. +15% damage.", "Mixed", [E("EarlyFire", chance=0.25, skip=0.4), SM("Damage", 1.15)])
q("old_grudge", "Old Grudge", "+35% damage against the part's former owner.", "Positive", [E("Grudge", 1.35)])
q("hollow", "Hollow", "-25% weight, -15% HP.", "Mixed", [SM("Weight", 0.75), SM("Hp", 0.85)])
q("leaking_core", "Leaking Core", "Stamina drains while idle, but recovers faster.", "Mixed", [E("IdleDrain", 4.0), SM("StaminaRegen", 1.25)])
q("remembers_sea", "Remembers the Sea", "Swims 50% faster, 10% slower on land.", "Mixed", [SM("SwimSpeed", 1.5), SM("LandSpeed", 0.9)], side="Saint")
q("dodges_left", "Dodges Left", "Left dodges gain 4 extra i-frames.", "Positive", [E("DodgeIframes", E("Left"), 4)])
q("dodges_right", "Dodges Right", "Right dodges gain 4 extra i-frames.", "Positive", [E("DodgeIframes", E("Right"), 4)])
q("heavy_handed", "Heavy-Handed", "+25% posture damage, attacks 10% slower.", "Mixed", [SM("PostureDamage", 1.25), SM("AttackSpeed", 0.9)])
q("glass_jaw", "Glass Jaw", "-25% posture.", "Negative", [SM("Posture", 0.75)])
q("thick_plating", "Thick Plating", "+30% armour, -8% speed.", "Mixed", [SM("Armor", 1.3), SM("Speed", 0.92)])
q("bloodthirsty", "Bloodthirsty", "Heal 6% of damage dealt.", "Positive", [E("Leech", 0.06)])
q("cracked_casing", "Cracked Casing", "-15% HP.", "Negative", [SM("Hp", 0.85)])
q("overclocked", "Overclocked", "Attacks 12% faster, -15% max stamina.", "Mixed", [SM("AttackSpeed", 1.12), SM("StaminaMax", 0.85)])
q("cold_steady", "Cold and Steady", "Posture recovers 40% faster.", "Positive", [E("Steady", 1.4)])
q("last_stand", "Last Stand", "+30% damage below 30% HP.", "Positive", [E("Desperate", below=0.3, mul=1.3)])
q("jittery_servos", "Jittery Servos", "Controls drift slightly.", "Negative", [E("Drift", 0.15)])
q("keen_sight", "Keen Sight", "+30% lock-on range.", "Positive", [SM("LockRange", 1.3)])
q("parry_memory", "Parry Memory", "Parry window +3 frames.", "Positive", [E("ParryWindow", 3)])
q("sluggish", "Sluggish", "-20% turn speed.", "Negative", [SM("TurnSpeed", 0.8)])
q("light_footed", "Light-Footed", "+8% speed.", "Positive", [SM("Speed", 1.08)])
q("barnacled", "Barnacled", "+15% armour, +15% weight.", "Mixed", [SM("Armor", 1.15), SM("Weight", 1.15)])
q("weeping_seam", "Weeping Seam", "Lose 1.5 HP per second.", "Negative", [E("Bleed", 1.5)])
q("hungry_metal", "Hungry Metal", "+10% damage; Hunger drains 30% faster.", "Mixed", [E("Famine", 1.3), SM("Damage", 1.1)])
q("ember_veins", "Ember Veins", "Attacks ignite.", "Positive", [E("Ignite")])
q("stubborn", "Stubborn", "+20% posture.", "Positive", [SM("Posture", 1.2)])
q("brittle", "Brittle", "-20% armour.", "Negative", [SM("Armor", 0.8)])
q("scavenger", "Scavenger", "+20% currency from kills.", "Positive", [E("Greed", 1.2)])
q("harvesters_hand", "Harvester's Hand", "+25% chance of extra harvest drops.", "Positive", [E("Harvester", 1.25)])
q("quick_tissue", "Quick Tissue", "Heals restore 20% more.", "Positive", [SM("HealPower", 1.2)])
q("scarred_nerves", "Scarred Nerves", "+15% guard, -5% speed.", "Mixed", [SM("Guard", 1.15), SM("Speed", 0.95)])
q("phantom_limb", "Phantom Limb", "+10% damage; sometimes you see things.", "Mixed", [SM("Damage", 1.1), E("Hallucinate", 0.05)])
q("tide_drunk", "Tide-Drunk", "+30% swim speed, -10% stamina regen.", "Mixed", [SM("SwimSpeed", 1.3), SM("StaminaRegen", 0.9)])
q("deep_roots", "Deep Roots", "+15% posture, -10% speed.", "Mixed", [SM("Posture", 1.15), SM("Speed", 0.9)], side="Kaiju")
q("rattling", "Rattling", "-10% lock range, +10% damage.", "Mixed", [SM("LockRange", 0.9), SM("Damage", 1.1)])
q("dead_weight", "Dead Weight", "+20% weight.", "Negative", [SM("Weight", 1.2)])
q("lean_build", "Lean Build", "-10% weight, -5% armour.", "Mixed", [SM("Weight", 0.9), SM("Armor", 0.95)])
q("sea_sickness", "Sea-Sick", "-20% swim speed.", "Negative", [SM("SwimSpeed", 0.8)], side="Kaiju")
q("static_singing", "Static Singing", "Imported part. +20% lock range, posture steadies.", "Positive", [SM("LockRange", 1.2), E("Steady", 1.2)], import_only=True)
q("friends_mark", "Friend's Mark", "Imported part. +8% damage and HP.", "Positive", [SM("Damage", 1.08), SM("Hp", 1.08)], import_only=True)
q("borrowed_luck", "Borrowed Luck", "Imported part. Parry window +2, dodges +2 i-frames.", "Positive", [E("ParryWindow", 2), E("DodgeIframes", E("Any"), 2)], import_only=True)
# special (never rolled)
q("rusted", "Rusted", "The city drowned. -15% armour.", "Negative", [SM("Armor", 0.85), E("Marker", "rusted")], special=True)
q("crumbling", "Crumbling", "The city stood. -15% HP.", "Negative", [SM("Hp", 0.85), E("Marker", "crumbling")], special=True)
q("corrupted", "Corrupted", "Carried through the Corrupted Block. +10% damage, unreliable.", "Mixed", [SM("Damage", 1.1), E("Hallucinate", 0.04), E("Marker", "corrupted")], special=True)
q("demo_curse", "Demo Disc Curse", "From a monster that was never in the game. +25% damage, bleeds.", "Mixed", [SM("Damage", 1.25), E("Bleed", 1.0)], special=True)
q("bounty_mark", "Bounty Mark", "Contract reward. +40% currency, +5% damage.", "Positive", [E("Greed", 1.4), SM("Damage", 1.05)], special=True)
q("oathbound", "Oathbound", "Contract reward. +15% posture, parry +2.", "Positive", [SM("Posture", 1.15), E("ParryWindow", 2)], special=True)
q("untouched", "Untouched", "Contract reward. +12% damage while above 90% HP.", "Positive", [E("Desperate", below=1.01, mul=1.06), SM("Damage", 1.06)], special=True)
q("severer", "Severer's Edge", "Contract reward. +20% posture damage.", "Positive", [SM("PostureDamage", 1.2)], special=True)
q("heretic_instability", "Heretic Instability", "Over half of this body is wrong. Drift and visions.", "Negative", [E("Drift", 0.12), E("Hallucinate", 0.08)], special=True)
write("quirks.ron", Q, "Quirks. `special` quirks are never rolled randomly (endings, corruption, contracts, events).\nAt least 30 rollable quirks are required by validation.")

# ---------------------------------------------------------------- chassis
CH = []
def ranks(trait_effect, pal, lore_reward):
    return [
        dict(rank=1, reward="License issued"),
        dict(rank=2, reward="Alternate palette", palette=T((c3(*pal[0]), c3(*pal[1])))),
        dict(rank=3, reward="Alternate starting loadout"),
        dict(rank=4, reward="+2% permanent damage", bonus=E("StatMul", E("Damage"), F(1.02))),
        dict(rank=5, reward="Signature trait unlocked", wins_against=3),
        dict(rank=6, reward="Lore entry: " + lore_reward),
        dict(rank=7, reward="+2% permanent HP", bonus=E("StatMul", E("Hp"), F(1.02))),
        dict(rank=8, reward="Gilded palette", palette=T((c3(200, 170, 80), c3(60, 50, 30)))),
        dict(rank=9, reward="+1% permanent posture", bonus=E("StatMul", E("Posture"), F(1.01))),
        dict(rank=10, reward="Master stencil and title"),
    ]
def ch(id, name, side, play, rival, parts, alt, pal, unlock, sig, sig_eff, lore, lore_reward):
    CH.append(dict(id=id, name=name, side=E(side), playstyle=play, rival=rival, parts=T(parts), alt_parts=[T(a) for a in alt],
                   palette=T((c3(*pal[0]), c3(*pal[1]))), unlock=unlock, signature=sig, signature_effect=sig_eff,
                   ranks=ranks(sig_eff, [(pal[1]), (pal[0])], lore_reward), lore=lore))
ch("iron_saint", "IRON SAINT", "Saint", "Sword and shield, balanced", "leviathan", IRON,
   [[IRON[0], VESPER[1], IRON[2], IRON[3], IRON[4], IRON[5], IRON[6]]], [(150, 150, 160), (90, 92, 100)], E("Always"),
   "Vigil: parries restore a sliver of heat headroom.", E("ParryWindow", 1), "The first Saint. Its pilot volunteered. Its pilot always volunteers.", "The Volunteer")
ch("st_cinder", "ST. CINDER", "Saint", "Heavy flamethrowers, Heat stamina", "goliath_moth", CINDER,
   [[CINDER[0], IRON[1], CINDER[2], CINDER[3], CINDER[4], CINDER[5], CINDER[6]]], [(120, 60, 40), (70, 62, 58)], E("Always"),
   "Kiln Blood: attacks ignite.", E("Ignite"), "Built in the Foundry Quarter from a furnace that never went out.", "The Furnace That Never Went Out")
ch("st_wren", "ST. WREN", "Saint", "Fast grappling-hook skirmisher, parry-focused", "pale_choir", WREN,
   [[WREN[0], WREN[1], WREN[2], WREN[3], WREN[4], WREN[5], IRON[6]]], [(70, 110, 140), (40, 50, 60)], E("Always"),
   "Featherweight: dodges gain 2 i-frames.", E("DodgeIframes", E("Any"), 2), "The smallest Saint. It counts the Choir's bodies out loud.", "Counting")
ch("st_anchor", "ST. ANCHOR", "Saint", "Harbor mech tethered to the city power grid", "mawgrave", ANCHOR,
   [[ANCHOR[0], ANCHOR[1], IRON[2], ANCHOR[3], ANCHOR[4], ANCHOR[5], ANCHOR[6]]], [(60, 80, 70), (150, 140, 60)], E("Always"),
   "Grid Tether: stamina regen doubles at full power, halves in blackout.", E("StatMul", E("StaminaRegen"), F(1.05)), "It cannot leave the harbor. The cable is the length of its love.", "The Cable")
ch("st_null", "ST. NULL", "Saint", "A hollow frame that copies the last part it struck", "tide_widow", NULL, [], [(30, 30, 36), (60, 58, 66)],
   E("SeverHeads", of=["iron_saint", "st_cinder", "st_wren", "st_anchor"], while_playing=E("Kaiju")),
   "Hollow Echo: copies the last struck part's light attack.", E("Marker", "mimic"), "When you took their heads, something took their place.", "Nobody Inside")
ch("leviathan", "LEVIATHAN", "Kaiju", "Coiling sea serpent, grabs and constricts", "iron_saint", LEV,
   [[LEV[0], LEV[1], LEV[2], LEV[3], LEV[4], LEV[5], MOTH[6]]], [(40, 70, 90), (150, 170, 150)], E("Always"),
   "Undertow: +15% swim speed.", E("StatMul", E("SwimSpeed"), F(1.15)), "It remembers the city when it was a reef.", "The Reef")
ch("goliath_moth", "GOLIATH MOTH", "Kaiju", "Flying spore clouds, drawn to fire and city lights", "st_cinder", MOTH,
   [[MOTH[0], MOTH[1], MOTH[2], MOTH[3], MOTH[4], MOTH[5], LEV[6]]], [(120, 110, 90), (70, 60, 50)], E("Always"),
   "Lure: +10% damage near fire and lit districts.", E("StatMul", E("Damage"), F(1.03)), "It flies at the Foundry every night. Every night, it burns.", "Every Night")
ch("pale_choir", "THE PALE CHOIR", "Kaiju", "Swarm; HP is body count; splits and recombines", "st_wren", CHOIR,
   [[CHOIR[0], CHOIR[1], CHOIR[2], CHOIR[3], CHOIR[4], CHOIR[5], CHOIR[6]]], [(225, 220, 210), (180, 170, 170)], E("Always"),
   "Many Throats: Blood refills 20% faster.", E("StatMul", E("StaminaRegen"), F(1.05)), "The drowned congregation of Cathedral Row, still singing.", "The Congregation")
ch("mawgrave", "MAWGRAVE", "Kaiju", "Burrower that ambushes from beneath the streets", "st_anchor", MAW,
   [[MAW[0], MAW[1], MAW[2], MAW[3], MAW[4], MAW[5], LEV[6]]], [(90, 80, 70), (60, 54, 48)], E("Always"),
   "Undercity: +15% posture.", E("StatMul", E("Posture"), F(1.05)), "It eats the cemeteries first. Nobody knows why. Nobody wants to.", "The Cemeteries")
ch("tide_widow", "THE TIDE-WIDOW", "Kaiju", "Leaves floods that heal it", "st_null", WIDOW, [], [(60, 80, 100), (200, 210, 220)],
   E("HarvestCores", of=["leviathan", "goliath_moth", "pale_choir", "mawgrave"], while_playing=E("Saint")),
   "Mourning Tide: heals while standing in floodwater.", E("Marker", "flood_heal"), "You took four hearts. One of them was hers.", "Four Hearts")
write("chassis.ron", CH, "Premade chassis. `parts` are in slot order: Frame, LeftArm, RightArm, Legs, Core, Head, Back.\nPremade chassis are always available; custom builds use harvested parts.")

# ---------------------------------------------------------------- enemies
EN = []
def en(id, name, side, arch, parts, hp_mul, scale, cur, react, col, best):
    EN.append(dict(id=id, name=name, side=E(side), archetype=E(arch), parts=T(parts), hp_mul=F(hp_mul), scale=F(scale),
                   currency=cur, reaction=react, color=c3(*col), bestiary=best))
D = [p["id"] for p in PARTS]
en("deacon", "DEACON FRAME", "Saint", "Brute", DEACON, 0.5, 0.75, 60, 16, (110, 110, 100), "Mass-produced militia Saints. Their pilots are paid by the hour.")
en("lancer", "LANCER FRAME", "Saint", "Gunner", [DEACON[0], LANCER[0], LANCER[1], DEACON[3], DEACON[4], LANCER[2], DEACON[6]], 0.45, 0.75, 70, 18, (90, 100, 110), "Deacons with rifles. They aim for the eyes.")
en("tank", "MILITIA TANK", "Saint", "Gunner", TANK, 0.5, 1.0, 50, 20, (90, 100, 70), "Harrowmouth's oldest defence. They still work, mostly.")
en("artillery", "PSALTER ARTILLERY", "Saint", "Battery", [TANK[0], ARTY[0], ARTY[1], TANK[3], TANK[4], TANK[5], TANK[6]], 0.45, 1.0, 70, 24, (100, 90, 70), "Long guns named after hymns. The shells are louder than the singing.")
en("drone", "SERAPH DRONE", "Saint", "Flyer", DRONE, 0.4, 1.0, 40, 16, (200, 200, 210), "Watch-drones with halos for rotors.")
en("aa_battery", "AA BATTERY", "Saint", "Battery", [AA[0], AA[1], AA[2], AA[3], TANK[4], AA[4], TANK[6]], 0.55, 1.0, 80, 22, (110, 110, 100), "Anti-air flak. Goliath Moth's worst night was a night with these.")
en("hull_leech", "HULL LEECH", "Kaiju", "Swarmer", LEECH, 0.35, 1.0, 30, 14, (90, 60, 70), "Parasites that cling to Saint hulls and drink the tissue beneath.")
en("brood_spawn", "BROOD SPAWN", "Kaiju", "Swarmer", [SPAWN[0], SPAWN[1], SPAWN[2], LEECH[3], SPAWN[3], LEECH[5], LEECH[6]], 0.3, 1.0, 20, 14, (60, 90, 80), "Spawn swarms. They come in tides.")
en("juvenile", "JUVENILE LEVIATHAN", "Kaiju", "Brute", JUV, 0.55, 1.0, 90, 16, (50, 90, 100), "Young serpents. Someone's children.")
en("brine_spitter", "BRINE SPITTER", "Kaiju", "Gunner", SPIT, 0.45, 1.0, 70, 18, (100, 110, 60), "Bile artillery on legs.")
en("shellback", "SHELLBACK", "Kaiju", "Brute", SHELL, 0.6, 1.0, 100, 18, (120, 80, 60), "Armoured crawlers. Break the shell, then the crab.")
en("gravelurk", "GRAVELURK", "Kaiju", "Ambusher", LURK, 0.5, 1.0, 90, 16, (80, 74, 64), "Smaller cousins of Mawgrave. They wait under manholes.")
en("dummy_saint", "TRAINING DUMMY (SAINT)", "Saint", "Dummy", IRON, 3.0, 1.0, 0, 30, (150, 150, 160), "A decommissioned Iron Saint shell, bolted to the floor.")
en("dummy_kaiju", "TRAINING DUMMY (KAIJU)", "Kaiju", "Dummy", LEV, 3.0, 1.0, 0, 30, (40, 70, 90), "A juvenile Leviathan held in a tank of brine. Don't feel bad.")
en("demo_kaiju", "PROTOTYPE-0", "Kaiju", "Brute", [SHELL[0], JUV[1], JUV[2], JUV[3], JUV[4], JUV[5], SHELL[6]], 1.2, 1.5, 300, 16, (255, 0, 255), "A monster from a demo disc. It was never in the finished game.")
en("demo_saint", "SAINT-ALPHA", "Saint", "Brute", [DEACON[0], DEACON[1], DEACON[2], DEACON[3], DEACON[4], LANCER[2], TANK[6]], 1.2, 1.5, 300, 16, (255, 0, 255), "A Saint from a demo disc. The texture on its face is a placeholder.")
write("enemies.ron", EN, "Minor enemies. Parts must match the enemy's side: drops are the enemy's own parts (Cross-Harvest).\n`reaction` is the AI's perception delay in ticks (no input reading).")

# ---------------------------------------------------------------- bosses
BS = []
def boss(id, name, title, side, parts, scale, hp_mul, phases, cur, intro, best, chassis=None, huge=False, weak=(), react=14):
    d = dict(id=id, name=name, title=title, side=E(side), parts=T(parts), scale=F(scale), hp_mul=F(hp_mul), huge=huge,
             weak_points=[dict(slot=E(s), height=F(h)) for s, h in weak],
             phases=[dict(at_hp=F(a), moves=[T((m, w)) for m, w in mvz], speed=F(sp), line=ln) for a, mvz, sp, ln in phases],
             currency=cur, reaction=react, intro=intro, bestiary=best)
    if chassis: d["chassis"] = chassis
    BS.append(d)
boss("boss_iron_saint", "IRON SAINT", "The Harbor Warden", "Saint", IRON, 1.15, 2.2, [
    (1.0, [("warden_combo", 4), ("warden_cleave", 3), ("warden_bash", 2)], 1.0, "WARDEN: Turn back to the sea."),
    (0.6, [("warden_combo", 3), ("warden_cleave", 3), ("warden_charge", 2), ("warden_feint", 2)], 1.1, "WARDEN: I have done this before. I will do this again."),
    (0.3, [("warden_combo", 3), ("warden_feint", 3), ("warden_charge", 3), ("warden_bash", 1)], 1.2, "WARDEN: ...who is in here with me?")],
    1200, "The first Saint stands in the surf where the ferries used to dock.", "Harbor Warden. Iron Saint chassis, factory loadout. Its feint is always the same length.", chassis="iron_saint")
boss("boss_st_cinder", "ST. CINDER", "The Furnace Saint", "Saint", CINDER, 1.15, 2.2, [
    (1.0, [("cinder_spray", 4), ("cinder_gout", 3)], 1.0, "CINDER: The Foundry does not close."),
    (0.6, [("cinder_spray", 3), ("cinder_gout", 3), ("cinder_mortar", 2)], 1.05, "CINDER: Burn clean. Burn clean."),
    (0.3, [("cinder_gout", 3), ("cinder_mortar", 2), ("cinder_vent", 3)], 1.1, "CINDER: VENTING.")],
    1300, "Furnace light. A Saint stands in it, unbothered.", "St. Cinder overheats after Vent Bloom. Punish the vent.", chassis="st_cinder")
boss("boss_st_wren", "ST. WREN", "The Rail Kestrel", "Saint", WREN, 1.05, 2.0, [
    (1.0, [("wren_jab", 4), ("wren_reel", 2)], 1.0, "WREN: One. Two. Three bodies. Four."),
    (0.6, [("wren_jab", 3), ("wren_reel", 3), ("wren_feint", 2)], 1.1, "WREN: You're not counting."),
    (0.3, [("wren_feint", 3), ("wren_dance", 3), ("wren_reel", 2)], 1.2, "WREN: I can't stop counting.")],
    1200, "Something small and fast perches on a signal gantry.", "St. Wren reels in from range; close the gap after the hook misses.", chassis="st_wren")
boss("boss_st_anchor", "ST. ANCHOR", "The Moored Saint", "Saint", ANCHOR, 1.2, 2.4, [
    (1.0, [("anchor_swing", 4), ("anchor_snare", 2)], 1.0, "ANCHOR: The cable holds."),
    (0.6, [("anchor_swing", 3), ("anchor_slam", 3), ("anchor_snare", 2)], 1.0, "ANCHOR: The grid holds."),
    (0.3, [("anchor_slam", 3), ("anchor_surge", 3), ("anchor_swing", 2)], 1.1, "ANCHOR: The city holds.")],
    1400, "A heavy Saint, chained to a pylon, guards the drowned market.", "St. Anchor is weaker in a blackout. Grid Surge has a ring telegraph.", chassis="st_anchor")
boss("boss_st_vesper", "ST. VESPER", "The Sleeping Saint", "Saint", VESPER, 1.4, 2.6, [
    (1.0, [("vesper_censer", 4), ("vesper_flare", 2)], 0.9, "The bell tolls once."),
    (0.6, [("vesper_censer", 3), ("vesper_toll", 3), ("vesper_flare", 2)], 1.0, "The bell tolls twice."),
    (0.3, [("vesper_toll", 3), ("vesper_rise", 2), ("vesper_censer", 2)], 1.1, "The bell tolls for you.")],
    1600, "The Cathedral's Saint wakes when the bell is rung.", "Climb its back when it kneels after The Sleeper Wakes.", huge=True, weak=[("Back", 0.75), ("Head", 0.95)])
boss("boss_lampwright", "THE LAMPWRIGHT", "Keeper of Lighthouse Point", "Saint", LAMPW, 1.4, 2.6, [
    (1.0, [("lamp_beam", 4), ("lamp_spear", 2)], 1.0, "LAMPWRIGHT: Lights on the water."),
    (0.6, [("lamp_beam", 3), ("lamp_spear", 3), ("lamp_salvo", 2)], 1.05, "LAMPWRIGHT: I see you. I always see you."),
    (0.3, [("lamp_blind", 2), ("lamp_spear", 3), ("lamp_salvo", 3)], 1.1, "LAMPWRIGHT: Look into the light.")],
    1500, "A Saint built into a lighthouse. Its lamp turns to find you.", "Its beam is a long sweep. Get behind it or under it.", huge=True, weak=[("Head", 0.95), ("Back", 0.7)])
boss("boss_colossus", "BREAKWATER COLOSSUS", "Wall Against the Sea", "Saint", COLOSSUS, 1.8, 3.2, [
    (1.0, [("colossus_fist", 4), ("colossus_sweep", 2)], 0.9, "The breakwater stands."),
    (0.66, [("colossus_fist", 3), ("colossus_slam", 3), ("colossus_sweep", 2)], 1.0, "The breakwater stands."),
    (0.33, [("colossus_slam", 3), ("colossus_flood", 2), ("colossus_sweep", 3)], 1.1, "The breakwater...")],
    3000, "The finale. The city's last wall walks.", "Huge. Climb to the Signal Tower and the Floodgate.", huge=True, weak=[("Head", 0.95), ("Back", 0.7), ("LeftArm", 0.6)])
boss("boss_leviathan", "LEVIATHAN", "Coil of the Harbor", "Kaiju", LEV, 1.3, 2.3, [
    (1.0, [("lev_lash", 4), ("lev_bite", 3)], 1.0, "The water in the harbor moves against the tide."),
    (0.6, [("lev_lash", 3), ("lev_bite", 3), ("lev_constrict", 2), ("lev_feint", 2)], 1.1, "It coils around the cranes."),
    (0.3, [("lev_feint", 3), ("lev_constrict", 2), ("lev_breach", 3)], 1.2, "It is not trying to win. It is trying to go home.")],
    1200, "Something enormous moves under the ferry docks.", "Leviathan's feint holds the bite. Dodge on the snap, not the rear.", chassis="leviathan")
boss("boss_goliath_moth", "GOLIATH MOTH", "Lantern of the Foundry", "Kaiju", MOTH, 1.3, 2.1, [
    (1.0, [("moth_buffet", 4), ("moth_dive", 2)], 1.0, "Wingbeats over the smokestacks."),
    (0.6, [("moth_buffet", 3), ("moth_dive", 3), ("moth_dust", 2)], 1.05, "The furnace light draws it lower."),
    (0.3, [("moth_bloom", 3), ("moth_dive", 3), ("moth_dust", 2)], 1.15, "The air turns to spores.")],
    1200, "It circles the Foundry's furnaces like a prayer.", "Goliath Moth is drawn to fire. During blackouts it loses its lure.", chassis="goliath_moth")
boss("boss_pale_choir", "THE PALE CHOIR", "Congregation of Cathedral Row", "Kaiju", CHOIR, 1.0, 2.2, [
    (1.0, [("choir_grasp", 4), ("choir_crush", 2)], 1.0, "Hymns from the flooded nave."),
    (0.6, [("choir_grasp", 3), ("choir_crush", 3), ("choir_split", 2)], 1.05, "The Choir splits into verses."),
    (0.3, [("choir_crush", 3), ("choir_recombine", 3), ("choir_split", 2)], 1.15, "The Choir sings as one.")],
    1200, "They file out of the cathedral in rows.", "HP is body count. Kill bodies while it is split; Recombine heals it.", chassis="pale_choir")
boss("boss_mawgrave", "MAWGRAVE", "Ossuary of the Rail Yards", "Kaiju", MAW, 1.3, 2.4, [
    (1.0, [("maw_rake", 4), ("maw_crush", 2)], 1.0, "The rails buckle from below."),
    (0.6, [("maw_rake", 3), ("maw_crush", 3), ("maw_ambush", 2)], 1.05, "It goes under."),
    (0.3, [("maw_ambush", 3), ("maw_quake", 3), ("maw_crush", 2)], 1.15, "The whole yard shakes.")],
    1300, "The rail yard's cemetery is empty. Something ate it.", "When it burrows, watch for the ground ring and move.", chassis="mawgrave")
boss("boss_wrecker", "THE WRECKER", "Lure of Lighthouse Point", "Kaiju", WRECKER, 1.5, 2.6, [
    (1.0, [("wreck_lash", 4), ("wreck_breaker", 2)], 1.0, "A false light bobs on the water."),
    (0.6, [("wreck_lash", 3), ("wreck_breaker", 3), ("wreck_lure", 2)], 1.05, "The light comes closer."),
    (0.3, [("wreck_feint", 3), ("wreck_lure", 2), ("wreck_breaker", 3)], 1.15, "It has wrecked bigger things than you.")],
    1500, "Ships ran aground here for a hundred years. This is why.", "Don't stare at the False Light. Its feint is long and always the same.", huge=True, weak=[("Head", 0.9), ("Back", 0.6)])
boss("boss_brine_matriarch", "BRINE MATRIARCH", "Queen of the Drowned Market", "Kaiju", MATRI, 1.6, 2.8, [
    (1.0, [("matri_snip", 4), ("matri_ram", 2)], 0.95, "Market bells ring underwater."),
    (0.6, [("matri_snip", 3), ("matri_slam", 3), ("matri_brood", 2)], 1.0, "The stalls on her back open for business."),
    (0.3, [("matri_slam", 3), ("matri_ram", 3), ("matri_brood", 2)], 1.1, "Everything must go.")],
    1600, "The market didn't drown. It was carried off.", "Climb the carapace when she slams. The brood sacs are a weak point.", huge=True, weak=[("Back", 0.85), ("Head", 0.7)])
boss("boss_tide_mother", "THE TIDE-MOTHER", "What the Sea Wants", "Kaiju", MOTHER, 2.0, 3.2, [
    (1.0, [("mother_lash", 4), ("mother_bite", 2)], 0.9, "The tide comes in."),
    (0.66, [("mother_lash", 3), ("mother_bite", 3), ("mother_maelstrom", 2)], 1.0, "The tide keeps coming in."),
    (0.33, [("mother_tide", 2), ("mother_maelstrom", 3), ("mother_bite", 3)], 1.1, "The tide never goes out.")],
    3000, "The finale. The sea stands up.", "Huge. Climb to the Pearl Crown.", huge=True, weak=[("Head", 0.95), ("Back", 0.6), ("Core", 0.5)])
write("bosses.ron", BS, "Handcrafted bosses: 7 per side (6 districts + the Breakwater finale).\nEvery damaging boss move needs a windup of at least 300 ms (18 ticks) - validated at load.")

# ---------------------------------------------------------------- districts
DIS = []
SE = ["hull_leech", "brood_spawn", "juvenile", "brine_spitter", "shellback", "gravelurk"]  # faced by saints
KE = ["deacon", "lancer", "tank", "artillery", "drone", "aa_battery"]  # faced by kaiju
def sv(fog, near, far, amb, sun, water, ruin, se, ke, mood, fort=False):
    return dict(fog=c3(*fog), fog_near=F(near), fog_far=F(far), ambient=c3(*amb), sun=c3(*sun), water=F(water), ruin=F(ruin),
                saint_enemies=se, kaiju_enemies=ke, mood=F(mood), fortifications=fort)
def district(id, name, lm, pos, seed, sea, pal, rev, mus, base_fog, sb, kb, start, lore, se, ke, finale=False, size=220.0):
    f = base_fog
    states = T((
        sv(shade(f, 1.1), 50, 190, (70, 70, 80), (255, 240, 220), -1.5, 0.05, se[:2], ke[:2] + ke[4:6], -0.2, True),
        sv(f, 40, 170, (60, 60, 70), (220, 210, 200), -1.0, 0.2, se[:3], ke[:3], 0.0),
        sv(shade(f, 0.7), 30, 140, (40, 50, 64), (150, 170, 200), 1.6, 0.35, se[1:4] + se[5:6], ke[2:5], 0.2),
        sv(shade(f, 0.55), 26, 130, (50, 44, 44), (180, 140, 120), 0.0, 0.75, se[3:6], ke[3:6], 0.35),
    ))
    DIS.append(dict(id=id, name=name, landmark=E(lm), map_pos=T((F(pos[0]), F(pos[1]))), size=F(size), block=F(36.0), seed=seed, sea=F(sea),
                    palette=[c3(*c) for c in pal], reverb=E(rev),
                    music=dict(tempo=F(mus[0]), root=mus[1], scale=mus[2], seed=mus[3]),
                    states=states, saint_boss=sb, kaiju_boss=kb, finale=finale, start=E(start), lore=lore))
MINOR = [0, 2, 3, 5, 7, 8, 10]; DORIAN = [0, 2, 3, 5, 7, 9, 10]; PHRYG = [0, 1, 3, 5, 7, 8, 10]; HARM = [0, 2, 3, 5, 7, 8, 11]; LYD = [0, 2, 4, 6, 7, 9, 11]
district("harbor", "THE HARBOR", "Docks", (0.22, 0.66), 1101, 0.3, [(90, 96, 104), (120, 110, 96), (70, 74, 70), (150, 140, 120)], "Hall", (96, 45, MINOR, 11),
         (90, 104, 120), "boss_leviathan", "boss_iron_saint", "Contested",
         ["The ferries stopped running in '94.", "Cranes lean over the water like mourners.", "Every Saint was carried off a ship here."], SE, KE)
district("cathedral_row", "CATHEDRAL ROW", "Cathedral", (0.48, 0.42), 2202, 0.15, [(110, 104, 100), (140, 130, 120), (80, 70, 70), (170, 160, 150)], "Hall", (72, 50, PHRYG, 22),
         (110, 100, 110), "boss_pale_choir", "boss_st_vesper", "Fortified",
         ["The bell has not rung since the first war.", "The congregation never left.", "Pews float in the nave on high tide."], SE, KE)
district("foundry_quarter", "FOUNDRY QUARTER", "Smokestacks", (0.70, 0.30), 3303, 0.1, [(100, 80, 70), (130, 100, 80), (70, 60, 56), (170, 120, 80)], "Room", (112, 43, HARM, 33),
         (120, 96, 80), "boss_goliath_moth", "boss_st_cinder", "Fortified",
         ["The furnaces have burned for ninety years.", "Moths the size of houses circle the stacks.", "Saint plating is still poured here by hand."], SE, KE)
district("old_rail_yards", "OLD RAIL YARDS", "Railyard", (0.78, 0.62), 4404, 0.05, [(96, 90, 80), (120, 112, 100), (66, 60, 56), (140, 120, 100)], "Studio", (104, 41, DORIAN, 44),
         (100, 96, 90), "boss_mawgrave", "boss_st_wren", "Contested",
         ["The rail cemetery was moved here in 1961. It was moved again in 1997, by something hungry.", "Signal lamps still change by themselves.", "Wren roosts on the gantries."], SE, KE)
district("lighthouse_point", "LIGHTHOUSE POINT", "Lighthouse", (0.12, 0.30), 5505, 0.45, [(80, 90, 100), (110, 120, 130), (60, 66, 70), (200, 190, 140)], "SpaceEcho", (84, 48, MINOR, 55),
         (80, 96, 116), "boss_wrecker", "boss_lampwright", "Flooded",
         ["The lamp turns even when nobody pays the keeper.", "A second light sometimes answers from the water.", "Shipwrecks here are older than the city."], SE, KE)
district("drowned_market", "THE DROWNED MARKET", "Market", (0.40, 0.80), 6606, 0.4, [(90, 100, 96), (130, 120, 100), (70, 80, 76), (160, 140, 110)], "SpaceEcho", (92, 46, PHRYG, 66),
         (80, 104, 104), "boss_brine_matriarch", "boss_st_anchor", "Flooded",
         ["Stalls still open at low tide. Nobody knows who runs them.", "St. Anchor's cable runs from here to the power station.", "The market bells ring underwater."], SE, KE)
district("breakwater", "THE BREAKWATER", "Seawall", (0.30, 0.94), 7707, 0.5, [(100, 100, 100), (130, 130, 124), (70, 70, 70), (150, 150, 150)], "Hall", (88, 40, HARM, 77),
         (90, 96, 106), "boss_tide_mother", "boss_colossus", "Contested",
         ["The last wall between Harrowmouth and the sea.", "Saint and Kaiju meet here at the end of every war.", "The tide between."], SE, KE, finale=True, size=200.0)
write("districts.ron", DIS, "Harrowmouth's districts. `states` are indexed Fortified, Contested, Flooded, Ruined.\nsaint_enemies are faced while playing Saint (so they must be kaiju-side), and vice versa.")

# ---------------------------------------------------------------- omens, disc rot, contracts, events, conditions, endings, lore, tuning
OM = []
def om(id, name, desc, reward, eff):
    OM.append(dict(id=id, name=name, desc=desc, reward=F(reward), effects=eff))
om("glass_hull", "Glass Hull", "You die in one hit.", 3.0, [E("GlassHull")])
om("famine", "Famine", "Hunger drains twice as fast; Saints' reactors run lean.", 1.4, [E("Famine", 2.0), E("StatMul", E("StaminaRegen"), F(0.85))])
om("black_signal", "Black Signal", "No HUD.", 1.5, [E("NoHud")])
om("riptide", "Riptide", "Every district is partly flooded.", 1.3, [E("Riptide")])
om("pilgrim", "Pilgrim", "No lock-on.", 1.5, [E("NoLock")])
om("echo", "Echo", "Enemies copy your last used move.", 1.6, [E("Echo")])
om("iron_tide", "Iron Tide", "Enemies have 40% more HP.", 1.4, [E("EnemyHp", F(1.4))])
om("red_moon", "Red Moon", "Enemies deal 30% more damage.", 1.5, [E("EnemyDamage", F(1.3))])
om("quickened", "Quickened", "Enemies move 15% faster.", 1.3, [E("EnemySpeed", F(1.15))])
om("thin_blood", "Thin Blood", "Heals restore half as much.", 1.4, [E("HealMul", F(0.5))])
om("undertow", "Undertow", "Death drops sink immediately.", 1.3, [E("NoRecovery")])
om("hallowed", "Hallowed Ground", "Your posture breaks faster.", 1.2, [E("StatMul", E("Posture"), F(0.75))])
write("omens.ron", OM, "Omens: stackable run modifiers with reward multipliers (Tide Runs, optional NG+).")
DR = []
rules = [("Enemies have +10% HP.", [E("EnemyHp", F(1.1))]), ("Enemies deal +10% damage.", [E("EnemyDamage", F(1.1))]),
         ("Heals restore 15% less.", [E("HealMul", F(0.85))]), ("Enemies move 8% faster.", [E("EnemySpeed", F(1.08))]),
         ("Your posture is 10% lower.", [E("StatMul", E("Posture"), F(0.9))]), ("Enemies have +15% more HP.", [E("EnemyHp", F(1.15))]),
         ("Enemies deal +15% more damage.", [E("EnemyDamage", F(1.15))]), ("Death drops sink.", [E("NoRecovery")]),
         ("Stamina recovers 10% slower.", [E("StatMul", E("StaminaRegen"), F(0.9))]), ("The disc is rotting. Enemies copy your moves.", [E("Echo")])]
for i, (r, e) in enumerate(rules):
    DR.append(dict(tier=i + 1, rule=r, effects=e))
write("disc_rot.ron", DR, "Disc Rot NG+ tiers 1-10. Each tier adds its rule on top of all lower tiers.")
CT = []
def ct(id, name, side, goal, rq, cur):
    d = dict(id=id, name=name, goal=goal, reward_quirk=rq, reward_currency=cur)
    d["side"] = E(side) if side else None
    CT.append(d)
ct("sever_left_arms", "Sever three left arms", None, E("SeverSlot", E("LeftArm"), 3), "severer", 400)
ct("sever_heads", "Take two heads", None, E("SeverSlot", E("Head"), 2), "keen_sight", 400)
ct("no_heal", "Win without healing", None, E("NoHeal"), "untouched", 500)
ct("flawless", "Win without taking damage", None, E("NoDamage"), "borrowed_luck", 900)
ct("parry_five", "Parry five attacks", None, E("Parries", 5), "oathbound", 450)
ct("cull_ten", "Destroy ten enemies", None, E("KillCount", 10), "bounty_mark", 350)
ct("fast", "Win in under four minutes", None, E("UnderTime", 14400), "light_footed", 400)
ct("gorge", "Eat eight buildings", "Kaiju", E("EatBuildings", 8), "bloodthirsty", 350)
ct("sever_legs", "Sever two locomotion parts", None, E("SeverSlot", E("Legs"), 2), "stubborn", 450)
write("contracts.ron", CT, "District contracts: optional bounties that pay in rare quirks.")
EV = []
def ev(id, name, kind, chance, desc, spawn=None, quirk=None):
    d = dict(id=id, name=name, kind=E(kind), chance=F(chance), desc=desc)
    if spawn: d["spawn"] = T(spawn)
    if quirk: d["quirk"] = quirk
    EV.append(d)
ev("demo_disc", "DEMO DISC INVASION", "DemoDisc", 0.08, "A monster that was never in the finished game has wandered in. Its parts are cursed.", ("demo_kaiju", "demo_saint"), "demo_curse")
ev("evacuation", "EVACUATION", "Evacuation", 0.12, "Civilian convoys are moving through the district. Saints escort them; Kaiju hunt them.")
ev("bell", "THE BELL", "CathedralBell", 0.06, "Someone rang the Cathedral bell. St. Vesper is awake.")
ev("static", "NIGHT OF STATIC", "NightOfStatic", 0.05, "The memory card hums. Every ghost walks tonight.")
ev("blackout", "BLACKOUT", "Blackout", 0.1, "The grid is down across Harrowmouth.")
ev("supply", "SUPPLY DROP", "SupplyDrop", 0.1, "A supply drop lands in the district: salvage for Saints, a meal for Kaiju.")
write("events.ron", EV, "Random war events rolled after each mission.")
CO = [dict(id="harbor_flooded", name="Harbor Flooded", desc="The harbor starts the war already Flooded.", kind=E("HarborFlooded"), wars=1),
      dict(id="one_saint", name="Last of Their Kind", desc="One chassis per side; no premade swaps.", kind=E("OneSaintPerSide"), wars=2),
      dict(id="all_contested", name="Open War", desc="Every district starts Contested.", kind=E("AllContested"), wars=1),
      dict(id="blackout_war", name="The Long Blackout", desc="The grid never comes back.", kind=E("Blackout"), wars=3, endings=["the_city_stands"]),
      dict(id="eternal_night", name="Eternal Night", desc="The sun never rises on Harrowmouth.", kind=E("EternalNight"), wars=3, endings=["drown_the_city"])]
write("conditions.ron", CO, "War Conditions unlocked through the War Record.")
ENDS = [dict(id="drown_the_city", name="DROWN THE CITY", card=["The sea came back for what was taken from it.", "Every Saint part on your memory card rusts."]),
        dict(id="the_city_stands", name="THE CITY STANDS", card=["Harrowmouth stands. The sea withdraws, for now.", "Every Kaiju part on your memory card crumbles."]),
        dict(id="become_the_saint", name="BECOME THE SAINT", card=["The monster climbs into the cockpit.", "It fits perfectly. It always did."]),
        dict(id="tide_between", name="THE TIDE BETWEEN", card=["Saint and Kaiju were never separate.", "Every part you ever harvested comes home to one body.", "The tide stops between the two shores."]),
        dict(id="disc3", name="DISC 3", card=["PLEASE INSERT DISC 3", "There is no Disc 3.", "There was always a Disc 3."])]
write("endings.ron", ENDS, "Endings.")
LORE = dict(loading=[
    "SALVAGE dropped on death becomes a wreck. Kaiju can strip it.",
    "BIOMASS dropped on death becomes a carcass. Saints can harvest it.",
    "Saint parts only drop while you play Kaiju. Kaiju parts only drop while you play Saint.",
    "Break an enemy's posture, then hit HEAVY on a locked part to SEVER it pristine.",
    "Resting advances the Tide Clock. High tide and night favour the Kaiju.",
    "A lopsided build leans. A leaning build drifts.",
    "Heretic grafts are powerful. Over half heretic unlocks HERETIC FORM.",
    "Every saved build can come back as a boss on the other side.",
    "The memory card has 15 blocks. Choose what to keep.",
    "Storms reveal hidden kaiju with every lightning strike.",
    "In a blackout, St. Anchor is weakened and Goliath Moth loses its lure.",
    "Telegraphs always have a sound. Listen.",
    "Parry: tap GUARD just before a hit lands.",
    "Light builds dash. Medium builds hop. Heavy builds shoulder-slide.",
], intro=["HARROWMOUTH, 1998.", "THE SEA HAS RISEN SIX TIMES.", "SIX TIMES THE SAINTS HAVE WALKED INTO THE SURF.", "THIS IS THE SEVENTH WAR."],
    sync_glitches=["SYNC 99%", "SYNC 9̶9̶%", "PILOT: [NO SIGNAL]", "TISSUE MATCH: 100%", "WHO IS DRIVING", "SALT IN THE COOLANT", "SYNC: YOU"],
    disc3=["The corrupted block remembers every war.", "Three wars. Three corruptions. One card.", "The tide was never the enemy.", "Neither were you."])
write("lore.ron", LORE, "Loading-screen lore, intro text, sync glitch strings and Disc 3 text.")
write("tuning.ron", dict(parry_window=8, steady_hands_parry=14, dodge_iframes=T((14, 12, 10)), dodge_distance=T((F(9.0), F(6.5), F(5.0))),
                         dodge_ticks=T((18, 22, 26)), sever_window=150, posture_break_ticks=150, vent_ticks=72, heal_ticks=48,
                         repair_heal=F(0.35), regrowth_heal=F(0.22), regrowth_cost=150, meta_cap=F(0.10), memory_blocks=15,
                         min_boss_windup=18, old_bones_speed=F(0.8)),
      "Global combat tuning. Dodge arrays are indexed [light dash, medium hop, heavy slide].")
print(len(PARTS), "parts", len(MOVES), "moves", len(Q), "quirks")
