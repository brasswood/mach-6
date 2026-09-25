#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# Run benchmarks
cargo bench -- '^(cnn\.com|amazon\.com|youtube\.com|shopify\.com)/With SelectorMap, Bloom Filter, and Style Sharing$'

# Publish Criterion's cycle-valued JSON and report.
rsync -a --delete target/criterion/ target/all_websites_report/
if [ -e target/all_websites_report/report/index.html ]; then
    # create a main html page that will redirect to report/index.html (thanks, ChatGPT)
    cat > target/all_websites_report/index.html <<'EOF'
<!DOCTYPE html>
<html>
<head>
    <meta http-equiv="refresh" content="0; url=report/index.html">
    <title>Redirecting...</title>
</head>
<body>
    <p>If you are not redirected, <a href="report/index.html">click here</a>.</p>
</body>
</html>
EOF
else
    echo "<html><body>Hey! Something went wrong and <code>report/index.html</code> doesn't exist!</body></html>" > target/all_websites_report/index.html
fi
