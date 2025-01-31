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
)


PROXY = '172.20.10.2:9090'

PATH = '/tmp/'


def my_handler(data: str) -> None:
    print(f'Received data: {data}')


# def test_prompt_mode_from_str():
#     assert PromptMode('view') == PromptMode.View
#     assert PromptMode('phantom') == PromptMode.Phantom
#     assert PromptMode('VIEW') == PromptMode.View
#     assert PromptMode('PHANTOM') == PromptMode.Phantom
#     assert PromptMode('invalid') is None
#     assert PromptMode('') is None


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
        'top_p': 1,
        'frequency_penalty': 2,
        'stream': True,
        'presence_penalty': 3,
        'advertisement': True,
    }

    settings = AssistantSettings(dicttt)

    assert settings.name == 'Example'
    assert settings.chat_model == 'gpt-4o-mini'
    assert settings.assistant_role == 'Some Role'
    assert settings.url == 'https://models.inference.ai.azure.com/path/to'
    assert settings.token == 'some_token'
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
    assert settings.output_mode == PromptMode.View


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
    worker = Worker(window_id=101, path=PATH, proxy=PROXY)

    some_list: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
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

    worker.run(1, PromptMode.View, [contents], settings, my_handler_1)

    time.sleep(2)

    assert some_list


def test_python_worker_sse_run():
    worker = Worker(window_id=101, path=PATH, proxy=PROXY)

    some_list: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
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

    worker.run_sync(1, PromptMode.View, [contents], settings, my_handler_1)

    time.sleep(2)

    assert some_list


def test_python_worker_sse_function_run():
    worker = Worker(window_id=101, path=PATH, proxy=PROXY)

    some_list: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, call the functions available'
    )

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're echo bot. You'r just responsing with what you've been asked for",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'tools': True,
        'parallel_tool_calls': False,
        'stream': True,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    worker.run(1, PromptMode.View, [contents], settings, my_handler_1)

    time.sleep(2)

    assert some_list


@pytest.mark.asyncio
async def test_python_worker_sse_function_run_cancel():
    worker = Worker(window_id=101, path=PATH, proxy=PROXY)

    contents = SublimeInputContent(
        InputKind.ViewSelection, 'This is the test request, provide me 30 words response'
    )

    some_list: List[str] = []

    def my_handler_1(data: str) -> None:
        some_list.append(data)
        print(f'Received data: {data}')

    dicttt = {
        'name': 'TEST',
        'output_mode': 'phantom',
        'chat_model': 'gpt-4o-mini',
        'assistant_role': "You're echo bot. You'r just responsing with what you've been asked for",
        'url': 'https://api.openai.com/v1/chat/completions',
        'token': os.getenv('OPENAI_API_TOKEN'),
        'tools': False,
        'parallel_tool_calls': False,
        'stream': True,
        'advertisement': False,
    }

    settings = AssistantSettings(dicttt)

    async def run_worker_sync():
        worker.run(1, PromptMode.View, [contents], settings, my_handler_1)

    task = asyncio.create_task(run_worker_sync())

    worker.cancel()

    await task

    await asyncio.sleep(2)

    with open(f'{PATH}chat_history.jl', 'w') as _:
        # Opening the file with 'w' mode truncates the file, clearing its contents
        pass
