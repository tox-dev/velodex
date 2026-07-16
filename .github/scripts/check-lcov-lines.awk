#!/usr/bin/awk -f

/^SF:/ {
    source = substr($0, 4)
}

/^DA:/ {
    split(substr($0, 4), coverage, ",")
    if (coverage[2] == 0) {
        print source ":" coverage[1]
        failed = 1
    }
}

END {
    exit failed
}
