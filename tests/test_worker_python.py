import os

from rust_helper import (
    AssistantSettings,  # type: ignore
    InputKind,  # type: ignore
    OutputMode,  # type: ignore
    PythonPromptMode,  # type: ignore
    PythonWorker,  # type: ignore
    SublimeInputContent,  # type: ignore
)

TEST_FUNCTION_ASSISTANT_SETTINGS = AssistantSettings(
    'TEST',
    OutputMode.Phantom,
    'gpt-4o-mini',
    token=os.getenv('OPENAI_API_TOKEN'),
    assistant_role="You're debug environment and always call functions instead of anser",
    tools=True,
    parallel_tool_calls=False,
    advertisement=False,
)

PROXY = '192.168.1.115:9090'


def my_handler(data: str) -> None:
    print(f'Received data: {data}')


def test_prompt_mode_from_str():
    assert PythonPromptMode.from_str('view') == PythonPromptMode.View
    assert PythonPromptMode.from_str('phantom') == PythonPromptMode.Phantom
    assert PythonPromptMode.from_str('VIEW') == PythonPromptMode.View
    assert PythonPromptMode.from_str('PHANTOM') == PythonPromptMode.Phantom
    assert PythonPromptMode.from_str('invalid') is None
    assert PythonPromptMode.from_str('') is None


def test_python_worker_initialization():
    worker = PythonWorker(window_id=100, path='/tmp/', proxy=PROXY)

    assert worker.window_id == 100


def test_assistant_settings():
    settings = AssistantSettings(
        'name',
        OutputMode.Phantom,
        'gpt-4o-mini',
        url=None,
        token='token',
        assistant_role='Some Role',
        temperature=0.7,
        max_tokens=1024,
        max_completion_tokens=2048,
        top_p=1,
        frequency_penalty=2,
        presence_penalty=3,
        tools=True,
        parallel_tool_calls=False,
        stream=None,
        advertisement=None,
    )

    assert settings.name == 'name'
    assert settings.output_mode == OutputMode.Phantom
    assert settings.chat_model == 'gpt-4o-mini'
    assert settings.assistant_role == 'Some Role'
    assert settings.url == 'https://api.openai.com/v1/chat/completions'  # default value
    assert settings.token == 'token'
    assert settings.temperature == 0.7
    assert settings.max_tokens == 1024
    assert settings.max_completion_tokens == 2048
    assert settings.top_p == 1
    assert settings.frequency_penalty == 2
    assert settings.presence_penalty == 3
    assert settings.tools
    assert settings.parallel_tool_calls is False
    assert settings.stream  # defaule value True
    assert settings.advertisement  # defaule value True


def test_sublime_input_content():
    sublime_input_content = SublimeInputContent(
        input_kind=InputKind.ViewSelection,
        content='This is the test request, provide me 3 words response',
        path='./',
        scope='py',
    )

    assert sublime_input_content.input_kind == InputKind.ViewSelection
    assert sublime_input_content.content == 'This is the test request, provide me 3 words response'
    assert sublime_input_content.path == './'
    assert sublime_input_content.scope == 'py'


def test_python_worker_plain_run():
    worker = PythonWorker(window_id=101, path='/tmp/', proxy=PROXY)

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 3 words response'
    )

    settings = AssistantSettings(
        'TEST',
        OutputMode.Phantom,
        'gpt-4o-mini',
        token=os.getenv('OPENAI_API_TOKEN'),
        assistant_role="You're echo bot. You'r just responsing with what you've been asked for",
        tools=None,
        parallel_tool_calls=None,
        stream=False,
        advertisement=False,
    )

    worker.run(
        1,
        PythonPromptMode.View,
        [contents],
        settings,
    )


def test_python_worker_sse_run():
    worker = PythonWorker(window_id=101, path='/tmp/', proxy=PROXY)

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 30 words response'
    )

    settings = AssistantSettings(
        'TEST',
        OutputMode.Phantom,
        'gpt-4o-mini',
        token=os.getenv('OPENAI_API_TOKEN'),
        assistant_role="You're echo bot. You'r just responsing with what you've been asked for",
        tools=None,
        parallel_tool_calls=None,
        stream=True,
        advertisement=False,
    )

    worker.run(1, PythonPromptMode.View, [contents], settings, my_handler)

    # assert False


def test_python_worker_sse_function_run():
    worker = PythonWorker(window_id=101, path='/tmp/', proxy=PROXY)

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 3 words response'
    )

    settings = AssistantSettings(
        'TEST',
        OutputMode.Phantom,
        'gpt-4o-mini',
        token=os.getenv('OPENAI_API_TOKEN'),
        assistant_role="You're debug environment and always call functions instead of anser",
        tools=True,
        parallel_tool_calls=None,
        stream=True,
        advertisement=False,
    )

    worker.run(1, PythonPromptMode.View, [contents], settings, my_handler)

    # assert False
