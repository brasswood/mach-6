#!/bin/bash
set -e -x

# Add cargo to $PATH
export PATH=~/.cargo/bin:$PATH

# Get the websites suite
git submodule update --init

# Run benchmarks
cargo bench

# make report/index.html accessible in the criterion/ directory
if [ -e target/criterion/report/index.html ]; then
    # create a symlink in target/criterion, which will point to "report/index.html" as interpreted from target/criterion
    cat > target/criterion/index.html <<'EOF'
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
    echo "<html><body>Hey! Something went wrong and <code>target/criterion/report/index.html</code> doesn't exist!</body></html>" > target/criterion/index.html
fi