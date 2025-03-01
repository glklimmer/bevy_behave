#!/bin/sh
# Generates the github README.md from the readme.inc.md
# 
# I don't keep the //! prefix on all lines in the inc file,
# since it makes it harder to edit, so re-add them temporarily:
(sed 's/^/\/\/\! /g' readme.inc.md > readme.inc.rs && cargo readme --no-license -i readme.inc.rs > README.md && rm readme.inc.rs)
