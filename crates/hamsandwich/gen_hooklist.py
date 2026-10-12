#!/usr/bin/env python3
# Generates src/hooklist.rs from AMX Mod X's hamsandwich sources (data in, Rust out):
#   python3 -I gen_hooklist.py <amxmodx>/modules/hamsandwich/hook_native.cpp \
#       <amxmodx>/plugins/include/ham_const.inc src/hooklist.rs
# The table follows `hooklist[]` in hook_native.cpp; names come from ham_const.inc's `Ham` enum
# (same order and numbers).
import re
import sys

src = open(sys.argv[1]).read()
start = src.index('hook_t hooklist[] =')
body = src[start:src.index('};', start)]
ents = re.findall(r'\{\s*V(_REMOVED)?\("([^"]+)"(?:,\s*([A-Za-z_]+))?\s*\)\s*\}', body)

inc = open(sys.argv[2]).read()
s0 = inc.index('enum Ham')
e1 = inc.index('HAM_LAST_ENTRY_DONT_USE_ME_LOL', s0)
blk = re.sub(r'/\*.*?\*/', '', inc[s0:e1], flags=re.S)
blk = re.sub(r'//[^\n]*', '', blk)
names = re.findall(r'(Ham_[A-Za-z0-9_]+)', blk)
assert len(names) == len(ents), (len(names), len(ents))

T = {'Void': 'Void', 'Int': 'Int', 'Bool': 'Bool', 'Short': 'Short', 'Float': 'Float', 'Vector': 'Vector',
     'pVector': 'PVector', 'pFloat': 'PFloat', 'Str': 'Str', 'Cbase': 'Cbase', 'Entvar': 'Entvar',
     'Edict': 'Edict', 'Trace': 'Trace', 'ItemInfo': 'ItemInfo'}
out = [
    "// Generated from AMX Mod X's modules/hamsandwich/hook_native.cpp (`hooklist`) and",
    "// plugins/include/ham_const.inc (`Ham` enum), AMX Mod X 1.10, Copyright (C) The AMX Mod X",
    "// Development Team. GPL-3.0-or-later with the AMX Mod X exceptions. Do not edit by hand:",
    "// regenerate with gen_hooklist.py.",
    "",
    "use crate::hooks::{Entry, Ty::*};",
    "",
    "/// Every `Ham` function in enum order: the plugin name, its hook name and signature.",
    "#[rustfmt::skip]",
    "pub const HOOKS: [Entry; %d] = [" % len(ents),
]
for (removed, hook, sig), name in zip(ents, names):
    if removed:
        out.append('    Entry::removed("%s", "%s"),' % (name, hook))
    elif sig == 'Deprecated':
        out.append('    Entry::deprecated("%s", "%s"),' % (name, hook))
    else:
        parts = sig.split('_')
        args = [T[p] for p in parts[1:] if p != 'Void']
        out.append('    Entry::new("%s", "%s", %s, &[%s]),' % (name, hook, T[parts[0]], ", ".join(args)))
out.append("];")
open(sys.argv[3], 'w').write("\n".join(out) + "\n")
