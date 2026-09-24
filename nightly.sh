#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# Google is archived in the controlled snapshot; expose it to the benchmark.
rm -rf websites/google.com
unzip -q websites/google.com.zip -d websites
trap 'rm -rf websites/google.com' EXIT

# Run benchmarks
cargo bench -- '^(cnn\.com|amazon\.com|google\.com|shopify\.com)/With SelectorMap and Bloom Filter$'

# copy criterion report to its own report directory
rsync -a --delete target/criterion/ criterion_report/
if [ -e criterion_report/report/index.html ]; then
    # create a main html page that will redirect to report/index.html (thanks, ChatGPT)
    cat > criterion_report/index.html <<'EOF'
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
    echo "<html><body>Hey! Something went wrong and <code>criterion_report/report/index.html</code> doesn't exist!</body></html>" > criterion_report/index.html
fi
