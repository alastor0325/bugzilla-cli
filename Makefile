.PHONY: install install-dev test test-all lint format check clean

install:
	pip install -r requirements.txt

install-dev:
	pip install -r requirements-dev.txt
	pre-commit install

test:
	pytest tests/ -x -q --ignore=tests/test_integration.py

test-all:
	pytest tests/ -x -q

lint:
	ruff check bugzilla_cli.py tests/

format:
	ruff format bugzilla_cli.py tests/

check: lint test

clean:
	rm -rf __pycache__ .pytest_cache .coverage htmlcov
	find . -name "*.pyc" -delete
