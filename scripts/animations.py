#!/usr/bin/env python3
"""Deterministic animation replay, exact cells and a separate render-time budget."""
import argparse
import copy
from reference import REFERENCE, compare, freeze, load_reference, render
from ui_cases import case


def cases():
    result=[]
    for language in ('ko','en'):
        for group,changes in [
            ('left',dict(left_open=False)), ('right',dict(right_open=False)),
            ('selection',dict(cursors=dict(main=1,right=0))), ('focus',dict(focus='right')),
            ('theme',dict(style=3)), ('transparent',dict(transparent=True)),
            ('help',dict(help=True)), ('language',dict(language='en' if language=='ko' else 'ko')),
            ('queue',dict(panel='queue',focus='right')), ('reduced',dict(left_open=False,motion=False)),
        ]:
            for index,elapsed in enumerate((0.0,0.01,0.026,0.042,0.09,0.17,0.27,0.4,0.8)):
                item=case(f'{language}-{group}-{index}',language,motion=True,panel_sizes=[21.0,29.0])
                item['group']=f'{language}-{group}'
                item['presentation'].update(now=1.0+elapsed,transition_time=1.01)
                if index: item['presentation'].update(copy.deepcopy(changes))
                result.append(item)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--freeze',action='store_true')
    args=parser.parse_args()
    inputs=cases()
    path=REFERENCE/'animations.json.gz'
    if args.freeze: freeze(path,inputs,animated=True)
    reference=load_reference(path,inputs)
    measured=render(inputs,animated=True,benchmark=8)
    compare(reference,measured)
    samples=sorted(time for frame in measured for time in frame['renderMs'])
    p95=samples[int((len(samples)-1)*0.95)]
    assert p95 <= 16.7, f'Render p95 {p95:.2f}ms exceeds 16.7ms'
    print(f'Render p95: {p95:.2f}ms ({len(samples)} samples)')


if __name__ == '__main__': main()
