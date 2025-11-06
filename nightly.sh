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
    ln -sf report/index.html target/criterion/
    sed -i '/<\/title>/ a\
    <base href="report/">
' target/criterion/index.html 
else
    echo "<html><body>Hey! Something went wrong and <code>target/criterion/report/index.html</code> doesn't exist!</body></html>" > target/criterion/index.html
fi