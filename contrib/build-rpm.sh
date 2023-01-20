#!/usr/bin/bash

mkdir -p ~/rpmbuild/SOURCES

cd ..

cargo package --allow-dirty
mv target/package/selinux-cascade-*.crate ~/rpmbuild/SOURCES

if [ ! -f  ~/rpmbuild/SOURCES/selinux-cascade-vendor.tar.gz ]; then

    if [ ! -d vendor ]; then
        cargo vendor
    fi

    for i in sexp clap clap_derive clap_mangen clap_lex quick-xml is-terminal roff hermit-abi; do
        tar -r -f contrib/selinux-cascade-vendor.tar vendor/$i
    done

    tar -r -f contrib/selinux-cascade-vendor.tar vendor/windows*
    gzip contrib/selinux-cascade-vendor.tar
    mv contrib/selinux-cascade-vendor.tar.gz ~/rpmbuild/SOURCES
    rm -rf vendor
fi

cd -

rm ~/rpmbuild/SRPMS/rust-selinux-cascade-*.src.rpm

snapshot=$(date +%Y%m%d%H%M%S)
sed -i "s/-s snapshot/-s $snapshot/" rust-selinux-cascade.spec
rpmbuild -bs rust-selinux-cascade.spec

mock -r fedora-rawhide-x86_64 --no-clean --rebuild ~/rpmbuild/SRPMS/rust-selinux-cascade-*.src.rpm
