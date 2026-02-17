#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# Run benchmarks
cargo bench

# copy criterion report to its own report directory
rsync -a --delete --delete-excluded target/criterion/ criterion_report/
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