.PHONY: generate clean test

# Regenerate src/wacp/v1.py from ../proto/*.proto using betterproto's protoc plugin.
#
# Requires:
#   - protoc (>=28; CI uses 29.3)
#   - betterproto[compiler] (provides `protoc-gen-python_betterproto`):
#       pip install "betterproto[compiler]>=1.2.5,<2"
#
# The plugin writes wacp/v1.py (matching the proto package `wacp.v1`) into a
# temp dir; we copy only that file to src/ to avoid clobbering src/wacp/__init__.py.
generate:
	@TMP=$$(mktemp -d) && \
	protoc --proto_path=../proto --python_betterproto_out=$$TMP \
		../proto/primitives.proto ../proto/agent.proto ../proto/highway.proto \
		../proto/coordinator.proto ../proto/taxonomy.proto && \
	cp $$TMP/wacp/v1.py src/wacp/v1.py && \
	rm -rf $$TMP && \
	echo "Regenerated src/wacp/v1.py from ../proto/"

clean:
	rm -rf .pytest_cache build dist *.egg-info src/wacp.egg-info
	find . -type d -name __pycache__ -exec rm -rf {} +

test:
	python -m pytest tests/ -v
