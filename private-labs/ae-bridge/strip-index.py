#!/usr/bin/env python3
"""Retire les balises du pont AE de src/index.html.

Suppression LIGNE À LIGNE plutôt que par bloc regex : une regex ancrée sur le
commentaire marqueur ne peut plus rien retirer si un nettoyage précédent a
consommé ce marqueur en laissant les <script> derrière lui — ce qui est
exactement arrivé en testant le cycle complet. Chaque ligne se juge seule, donc
c'est idempotent et ça ne peut pas laisser d'orphelin.
"""
import sys

NEEDLES = ('ae-bridge-dev', 'aescript-ui.js', 'aescript-host.js',
           'aeext-host.js', 'ae-bridge.css')

path = sys.argv[1]
lines = open(path, encoding='utf-8').read().splitlines(True)
kept = [l for l in lines if not any(n in l for n in NEEDLES)]
open(path, 'w', encoding='utf-8').writelines(kept)
print('  index.html : %d ligne(s) retirée(s)' % (len(lines) - len(kept)))
