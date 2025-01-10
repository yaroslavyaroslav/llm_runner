import os

from rust_helper import PythonPromptMode, PythonWorker  # type: ignore


def test_prompt_mode_from_str():
    assert PythonPromptMode.from_str('view') == PythonPromptMode.View
    assert PythonPromptMode.from_str('phantom') == PythonPromptMode.Phantom
    assert PythonPromptMode.from_str('VIEW') == PythonPromptMode.View
    assert PythonPromptMode.from_str('PHANTOM') == PythonPromptMode.Phantom
    assert PythonPromptMode.from_str('invalid') is None
    assert PythonPromptMode.from_str('') is None


def test_python_worker_initialization():
    worker = PythonWorker(window_id=100, path='/tmp/')

    assert worker.window_id == 100


def test_python_worker_plain_run():
    worker = PythonWorker(window_id=101, path='/tmp/', proxy='172.20.10.2:9090')

    token = os.getenv('OPENAI_API_TOKEN')

    worker.run(
        1,
        PythonPromptMode.View,
        '[{ "content": "This is the test request, provide me 3 words response", "input_kind": "view_selection" }]',
        '{"advertisement": true,'
        + '"chat_model": "gpt-4o-mini",'
        + '"name": "Some Name",'
        + '"output_mode": "Panel",'
        + '"stream": false,'
        + f'"token": "{token}",'
        + '"url": "https://api.openai.com/v1/chat/completions" }',
    )


def test_python_worker_sse_run():
    worker = PythonWorker(window_id=101, path='/tmp/', proxy='172.20.10.2:9090')

    token = os.getenv('OPENAI_API_TOKEN')

    worker.run(
        1,
        PythonPromptMode.View,
        '[{ "content": "This is the test request, provide me 3 words response", "input_kind": "view_selection" }]',
        '{"advertisement": true,'
        + '"chat_model": "gpt-4o-mini",'
        + '"name": "Some Name",'
        + '"output_mode": "Panel",'
        + '"stream": true,'
        + f'"token": "{token}",'
        + '"url": "https://api.openai.com/v1/chat/completions" }',
    )
