import rust_helper
import pytest
import sys


@pytest.fixture
def mock_sublime_module():
    class MockSublimeModule:
        @staticmethod
        def load_settings(string):
            return {"some_key": "some_value"}

    sys.modules["sublime"] = MockSublimeModule
    yield
    del sys.modules["sublime"]


def test_load_settings(mock_sublime_module):
    settings = rust_helper.load_settings("sublime", "example_string")
    assert isinstance(settings, rust_helper.Settings)
    value = settings.get("some_key")
    assert value == "some_value"


def test_settings_get(mock_sublime_module):
    settings = rust_helper.load_settings("sublime", "example_string")
    value = settings.get("some_key")
    assert value == "some_value"


def test_settings_class(mock_sublime_module):
    settings = rust_helper.load_settings("sublime", "example_string")
    assert isinstance(settings, rust_helper.Settings)
