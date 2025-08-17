
set -x
set -eo pipefail
if ![ -x "$(command -v sqlx)"]; then
	echo >&2 "sqlx is not installed, install it with cargo install sqlx-cli"
	exit 1
fi

DB_FILE=db.sqlite

export DATABASE_URL=sqlite://$DB_FILE
echo "Using: $DATABASE_URL"

sqlx database create
sqlx migrate run
