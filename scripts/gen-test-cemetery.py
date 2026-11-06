#!/usr/bin/env python3
"""Generates a large, fictional test cemetery ("Nagyvölgyi köztemető") as CSV
files for the TEMISZ import (Import és export › Excel/CSV import).

Every person is made up. Rows are in chronological order with folio numbers,
so the register, the rest-period rules and the expiries look realistic:
coffin burials, reburials after the rest period, urns placed into graves,
urn niches, double graves, child graves, a few honorary graves, holders with
address and phone, leases (many of them expired).

Usage: python3 scripts/gen-test-cemetery.py [out_dir] [rows_per_file]
"""
import csv
import os
import random
import sys
from datetime import date, timedelta

random.seed(20261106)
OUT = sys.argv[1] if len(sys.argv) > 1 else "samples/nagyvolgy"
PER_FILE = int(sys.argv[2]) if len(sys.argv) > 2 else 1000

FAMILY = """Nagy Kovács Tóth Szabó Horváth Varga Kiss Molnár Németh Farkas Balogh Papp Takács Juhász Lakatos Mészáros Oláh Simon Rácz Fekete
Szilágyi Török Fehér Balázs Gál Kis Szűcs Kocsis Orsós Pintér Fodor Szalai Sipos Magyar Lukács Gulyás Biró Király Katona László Jakab Bogdán Balog
Sándor Boros Fazekas Kelemen Antal Orosz Somogyi Fülöp Veres Budai Vincze Hegedűs Deák Pap Bálint Illés Pálfi Vass Lengyel Szőke Fábián Bodnár Hajdu
Halász Jónás Kozma Máté Székely Gáspár Pásztor Bakos Dudás Major Virág Hegyi Orbán Kántor Barta Lengyel Csonka Szekeres Benkő Vida Kun Bencze Erdős
Csizmadia Bozsó Ambrus Szénási Rigó Sárközi Kardos Zsigmond Borbély Vadász Csizmár Füredi Kálmán Medve Pásztor Bognár Gergely Ács Tamás Rozsnyai""".split()
MALE = """József János István László Ferenc Zoltán Sándor Gábor Mihály Attila Imre Tibor György Károly Béla Lajos Antal Pál Gyula Árpád Ernő Dezső Jenő
Géza Kálmán Miklós Endre Vilmos Rezső Lőrinc Bertalan Elemér Ottó Tamás Péter András Csaba Róbert Tivadar Szilveszter Lukács Márton Dénes""".split()
FEMALE = """Mária Erzsébet Ilona Katalin Éva Margit Julianna Anna Erika Zsuzsanna Magdolna Rozália Irén Terézia Gizella Piroska Edit Klára Ágnes Judit
Etelka Jolán Valéria Borbála Aranka Emília Hajnalka Ibolya Lenke Márta Sarolta Vilma Ottília Teréz Krisztina Andrea Mónika Gabriella Ildikó Beáta""".split()
TOWNS = """Nagyvölgy Kisvölgy Felsőmező Alsóhegy Révfalu Tölgyes Berekfürdő Szentmárton Hegyalja Kútvölgy Rétszilas Vámosmező Diósberény Erdőkertes""".split()
BIRTH = TOWNS + """Budapest Kecskemét Szeged Debrecen Pécs Győr Miskolc Székesfehérvár Szolnok Kaposvár Eger Veszprém Baja Kalocsa Nagykőrös Cegléd""".split()
STREETS = """Fő utca|Petőfi Sándor utca|Kossuth Lajos utca|Rákóczi utca|Dózsa György utca|Ady Endre utca|Arany János utca|Széchenyi tér|Béke utca|Templom köz|Kert utca|Rózsa utca|Tavasz utca|Malom utca|Iskola utca|Vasút utca""".split("|")
PLACES = ["otthonában", "Nagyvölgyi Városi Kórház", "Megyei Kórház", "Szent Erzsébet Idősek Otthona", "Kisvölgyi Rendelőintézet"]
TODAY = date(2026, 11, 1)

def name(sex):
    return random.choice(FAMILY), random.choice(MALE if sex == "male" else FEMALE)

def addr(town=None):
    t = town or random.choice(TOWNS)
    pc = 6000 + TOWNS.index(t) * 17 if t in TOWNS else 1000 + random.randint(0, 8000)
    return f"{pc} {t}, {random.choice(STREETS)} {random.randint(1, 120)}."

def phone():
    return f"+36 {random.choice(['20', '30', '70'])} {random.randint(100, 999)} {random.randint(1000, 9999)}"

def hu(d):
    return d.strftime("%Y.%m.%d.")

# ---- layout: sections, rows, plots, types
LAYOUT = [  # section, rows, plots per row, type mix
    ("A", 14, 30, [("single", 80), ("double", 15), ("child", 5)]),
    ("B", 14, 30, [("single", 75), ("double", 20), ("child", 5)]),
    ("C", 12, 28, [("single", 70), ("double", 30)]),
    ("D", 12, 28, [("single", 70), ("double", 25), ("child", 5)]),
    ("E", 10, 26, [("single", 60), ("double", 40)]),
    ("F", 10, 26, [("single", 65), ("double", 35)]),
    ("G", 8, 24, [("single", 85), ("double", 15)]),
    ("H", 4, 12, [("honorary", 100)]),
    ("U", 10, 30, [("urn_niche", 70), ("urn_grave", 30)]),
    ("K", 6, 20, [("child", 100)]),
]
TYPE_NAME = {"single": "Egyes sírhely", "double": "Kettős sírhely", "child": "Gyermeksír", "urn_niche": "Urnafülke",
             "urn_grave": "Urnasírhely", "honorary": "Díszsírhely"}
# sections were opened one after the other: older parts hold older graves
SECTION_START = {"A": 1955, "B": 1962, "H": 1965, "C": 1970, "K": 1972, "D": 1978, "E": 1986, "U": 1995, "F": 1994, "G": 2008}
FILL = {"A": .97, "B": .95, "C": .93, "D": .9, "E": .85, "F": .75, "G": .45, "H": .7, "U": .6, "K": .5}

def pick(mix):
    r = random.uniform(0, sum(w for _, w in mix))
    for t, w in mix:
        r -= w
        if r <= 0:
            return t
    return mix[-1][0]

def rand_date(y0, y1):
    a = date(y0, 1, 1)
    b = min(date(y1, 12, 31), TODAY - timedelta(days=5))
    return a + timedelta(days=random.randint(0, max(0, (b - a).days)))

events = []  # dicts, sorted later
for sec, rows, per_row, mix in LAYOUT:
    for r in range(1, rows + 1):
        for p in range(1, per_row + 1):
            if random.random() > FILL[sec]:
                continue
            ptype = pick(mix)
            ident = f"{sec}/{r}/{p}"
            family = random.choice(FAMILY)
            start = SECTION_START[sec] + (r - 1) * (2 if sec in "ABCDEF" else 1)
            first = rand_date(min(start, 2025), min(start + 6, 2026))
            burials = []
            if ptype in ("urn_niche", "urn_grave"):
                n = random.choice([1, 1, 2]) if ptype == "urn_niche" else random.choice([1, 2, 2, 3])
                d = first
                for _ in range(n):
                    burials.append((d, "urna"))
                    d += timedelta(days=random.randint(400, 6000))
            elif ptype == "child":
                burials.append((first, "child"))
            else:
                d = first
                coffins = 0
                cap = 2 if ptype in ("double", "honorary") else 1
                last_coffin = None
                for _ in range(random.choice([1, 1, 2, 2, 3, 4])):
                    if d > TODAY - timedelta(days=5):
                        break
                    if coffins < cap:
                        burials.append((d, "koporsó"))
                        coffins += 1
                        last_coffin = d
                    elif (d - last_coffin).days > 26 * 366 and random.random() < .5:
                        burials.append((d, "rátemetés"))
                        last_coffin = d
                    else:
                        burials.append((d, "urna"))
                    d += timedelta(days=random.randint(300, 9000))
            burials = [b for b in burials if b[0] <= TODAY - timedelta(days=5)]
            if not burials:
                continue
            # the holder: a relative with the family name, living nearby
            hsex = random.choice(["male", "female"])
            hfam, hgiven = family, random.choice(MALE if hsex == "male" else FEMALE)
            hold = f"{hfam} {hgiven}"
            haddr = addr(random.choice(TOWNS[:6]))
            hphone = phone() if random.random() < .7 else ""
            for i, (bd, kind) in enumerate(burials):
                sex = random.choice(["male", "female"])
                fam = family if random.random() < .8 else random.choice(FAMILY)
                giv = random.choice(MALE if sex == "male" else FEMALE)
                age = random.randint(1, 14) if kind == "child" else int(random.triangular(38, 99, 82))
                death = bd - timedelta(days=random.randint(3, 14))
                birth = death - timedelta(days=age * 365 + random.randint(0, 364))
                birth_name = ""
                if sex == "female" and age > 20 and random.random() < .6:
                    birth_name = f"{random.choice(FAMILY)} {giv}"
                    if random.random() < .5:
                        giv = f"{giv}"  # married women keep their own given name in the register
                mfam, mgiv = name("female")
                prefix = "özv. " if sex == "female" and age > 60 and random.random() < .12 else ("dr. " if random.random() < .02 else "")
                last = i == len(burials) - 1
                events.append({
                    "date": bd,
                    "Temetés napja": hu(bd),
                    "Esemény": {"koporsó": "koporsós temetés", "child": "koporsós temetés", "urna": "urnaelhelyezés", "rátemetés": "rátemetés"}[kind],
                    "Elhunyt neve": f"{prefix}{fam} {giv}",
                    "Születési név": birth_name,
                    "Nem": "férfi" if sex == "male" else "nő",
                    "Születési hely": random.choice(BIRTH),
                    "Születési idő": hu(birth) if random.random() > .03 else str(birth.year),
                    "Anyja neve": f"{mfam} {mgiv}" if random.random() > .04 else "",
                    "Utolsó lakóhely": addr(),
                    "Elhalálozás helye": random.choice(TOWNS[:5]) + ", " + random.choice(PLACES),
                    "Elhalálozás ideje": hu(death),
                    "Sírhely": ident,
                    "Helytípus": TYPE_NAME[ptype],
                    "Rendelkezési jogosult": hold,
                    "Jogosult lakcíme": haddr,
                    "Telefon": hphone,
                    # the current lease started with the latest burial
                    "Megváltás kezdete": hu(bd) if last else "",
                    "Megváltás ideje (év)": ("10" if ptype.startswith("urn") else "25") if last else "",
                    "Megjegyzés": "díszsírhely, a község halottja" if ptype == "honorary" and i == 0 else "",
                })

events.sort(key=lambda e: (e["date"], e["Sírhely"]))
cols = [k for k in events[0] if k != "date"]
os.makedirs(OUT, exist_ok=True)
for f in os.listdir(OUT):
    if f.endswith(".csv"):
        os.remove(os.path.join(OUT, f))
parts = [events[i:i + PER_FILE] for i in range(0, len(events), PER_FILE)]
folio = 0
for n, part in enumerate(parts, 1):
    path = os.path.join(OUT, f"nagyvolgy-{n:02d}-{part[0]['date'].year}-{part[-1]['date'].year}.csv")
    with open(path, "w", newline="", encoding="utf-8-sig") as fh:
        w = csv.writer(fh, delimiter=";")
        w.writerow(["Folyószám"] + cols)
        for e in part:
            folio += 1
            w.writerow([folio] + [e[c] for c in cols])
plots = len({e["Sírhely"] for e in events})
print(f"{len(events)} temetés, {plots} sírhely, {len(parts)} fájl -> {OUT}")
