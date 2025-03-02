import asyncio
import os
import time
from typing import List

import pytest
from llm_runner import (
    AssistantSettings,  # type: ignore
    InputKind,  # type: ignore
    PromptMode,  # type: ignore
    SublimeInputContent,  # type: ignore
    Worker,  # type: ignore
    ReasonEffort,  # type: ignore
    ApiType,  # type: ignore
)


PATH = '/tmp/'


def function_handeler(name: str, args: str) -> str:
    return 'Success'


def test_python_worker_initialization():
    worker = Worker(window_id=100, path=PATH)

    assert worker.window_id == 100


def test_assistant_settings():
    dicttt = {
        'name': 'Example',
        'output_mode': 'view',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': 'Some Role',
        'url': 'https://models.inference.ai.azure.com/path/to',
        'token': 'some_token',
        'tools': True,
        'parallel_tool_calls': False,
        'temperature': 0.7,
        'max_tokens': 1024,
        'max_completion_tokens': 2048,
        'top_p': 1.0,
        'frequency_penalty': 2.0,
        'reasoning_effort': 'low',
        'stream': False,
        'presence_penalty': 3.0,
        'advertisement': False,
        'api_type': 'open_ai',
    }

    settings = AssistantSettings(dicttt)

    assert settings.name == 'Example'
    assert settings.chat_model == 'gpt-4o-mini'
    assert settings.assistant_role == 'Some Role'
    assert settings.url == 'https://models.inference.ai.azure.com/path/to'
    assert settings.token == 'some_token'
    assert settings.temperature == 0.7
    assert settings.max_tokens is None  # due to max_completion_tokens presence
    assert settings.max_completion_tokens == 2048
    assert settings.top_p == 1
    assert settings.frequency_penalty == 2
    assert settings.presence_penalty == 3
    assert settings.tools
    assert settings.parallel_tool_calls is False
    assert settings.reasoning_effort == ReasonEffort.Low
    assert not settings.stream
    assert not settings.advertisement
    assert settings.output_mode == PromptMode.View
    assert settings.api_type == ApiType.OpenAi


def test_assistant_settings_real():
    dicttt = {
        'advertisement': False,
        'api_type': 'open_ai',
        'assistant_role': 'sdf',
        'chat_model': 'o3-mini',
        'name': 'o3-mini low',
        'reasoning_effort': 'low',
        'stream': True,
        'timeout': 20,
        'token': 'sk-proj-',
    }

    settings = AssistantSettings(dicttt)

    assert settings.name == 'o3-mini low'
    assert settings.chat_model == 'o3-mini'
    assert settings.assistant_role == 'sdf'
    assert settings.token == 'sk-proj-'
    assert settings.timeout == 20
    assert settings.reasoning_effort == ReasonEffort.Low
    assert settings.stream  # defaule value True
    assert settings.advertisement is False  # defaule value True
    assert settings.api_type == ApiType.OpenAi


def test_python_worker_plain_run():
    worker = Worker(window_id=101, path=PATH, proxy=os.environ.get('PROXY'))

    some_list: List[str] = []
    error_list: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    def error_handler_1(data: str) -> None:
        error_list.append(data)
        print(f'Received data: {data}')

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 3 words response'
    )

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're echo bot. You'r just responsing with what you've been asked for",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'stream': False,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    worker.run(1, PromptMode.View, [contents], settings, my_handler_1, error_handler_1, function_handeler)

    time.sleep(2)

    assert some_list


def test_python_worker_sse_run():
    worker = Worker(window_id=101, path=PATH, proxy=os.environ.get('PROXY'))

    some_list: List[str] = []
    some_errors: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    def error_handler_1(data: str) -> None:
        some_errors.append(data)
        print(f'Received data: {data}')

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 30 words response'
    )

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're echo bot. You'r just responsing with what you've been asked for",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'stream': True,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    worker.run_sync(
        1,
        PromptMode.View,
        [contents],
        settings,
        my_handler_1,
        error_handler_1,
        function_handeler,
    )

    time.sleep(2)

    assert some_list


def test_python_worker_sse_function_run():
    worker = Worker(window_id=101, path=PATH, proxy=os.environ.get('PROXY'))

    some_list: List[str] = []
    some_errors: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    def error_handler_1(data: str) -> None:
        some_errors.append(data)
        print(f'Received data: {data}')

    contents = SublimeInputContent(
        InputKind.ViewSelection,
        'This is the test request, call the read_region_content function on /tmp/some.txt',
    )

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're the function runner bot. You call a function and then prompt response to the user",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'tools': True,
        'parallel_tool_calls': False,
        'stream': True,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    worker.run(1, PromptMode.View, [contents], settings, my_handler_1, error_handler_1, function_handeler)

    time.sleep(2)

    assert some_list


@pytest.mark.asyncio
async def test_python_worker_sse_function_run_cancel():
    worker = Worker(window_id=101, path=PATH, proxy=os.environ.get('PROXY'))

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 30 words response'
    )

    some_list: List[str] = []
    some_errors: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    def error_handler_1(data: str) -> None:
        some_errors.append(data)
        print(f'Received data: {data}')

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're echo bot. You'r just responsing with what you've been asked for",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'tools': False,
        'stream': True,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    async def run_worker_sync():
        worker.run(1, PromptMode.View, [contents], settings, my_handler_1, error_handler_1, function_handeler)

    task = asyncio.create_task(run_worker_sync())

    worker.cancel()

    await task

    await asyncio.sleep(2)

    with open(f'{PATH}chat_history.jl', 'w') as _:
        # Opening the file with 'w' mode truncates the file, clearing its contents
        pass
