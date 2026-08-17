#!/usr/bin/env bash
set -euo pipefail

domain_tree="$(cargo tree -p babel-domain --prefix none)"
storage_tree="$(cargo tree -p babel-storage --prefix none)"
runtime_tree="$(cargo tree -p babel-runtime --prefix none)"
resource_graph_tree="$(cargo tree -p babel-resource-graph --prefix none)"
tir_tree="$(cargo tree -p babel-tir --prefix none)"
adapter_protocol_tree="$(cargo tree -p babel-adapter-protocol --prefix none)"
adapter_host_tree="$(cargo tree -p babel-adapter-host --prefix none)"

case "$domain_tree" in
  *babel-storage*|*babel-application*|*babel-runtime*)
    echo "babel-domain must not depend on storage, application, or runtime" >&2
    exit 1
    ;;
esac

case "$storage_tree" in
  *babel-application*|*babel-runtime*)
    echo "babel-storage must not depend on application or runtime" >&2
    exit 1
    ;;
esac

case "$runtime_tree" in
  *babel-application*|*babel-storage*)
    echo "babel-runtime must not depend on application or storage" >&2
    exit 1
    ;;
esac

case "$resource_graph_tree" in
  *babel-storage*|*babel-application*|*babel-runtime*|*babel-adapter-protocol*|*babel-adapter-host*)
    echo "babel-resource-graph must remain a pure contract below storage and adapters" >&2
    exit 1
    ;;
esac

case "$tir_tree" in
  *babel-storage*|*babel-application*|*babel-runtime*|*babel-adapter-protocol*|*babel-adapter-host*)
    echo "babel-tir must remain a pure contract below storage and adapters" >&2
    exit 1
    ;;
esac

case "$adapter_protocol_tree" in
  *babel-storage*|*babel-application*|*babel-runtime*|*babel-adapter-host*)
    echo "babel-adapter-protocol must not depend on storage, runtime, application, or host" >&2
    exit 1
    ;;
esac

case "$adapter_host_tree" in
  *babel-storage*|*babel-application*)
    echo "babel-adapter-host must not depend on authoritative storage or application" >&2
    exit 1
    ;;
esac

echo "architecture dependency direction: ok"
