#!/usr/bin/env bash
set -euo pipefail

sql-formatter -c <(cat << EOF
{
  "language": "sqlite",
  "tabWidth": 2,
  "keywordCase": "upper",
  "linesBetweenQueries": 1
}
EOF
) "$@"
