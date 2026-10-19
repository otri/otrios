# SPDX-License-Identifier: Apache-2.0
"""DRM timestamp parser tests; no device access, input data or root required."""
import importlib.util,pathlib,struct,unittest
p=pathlib.Path(__file__).with_name('drm-flip-events.py');s=importlib.util.spec_from_file_location('direct',p);direct=importlib.util.module_from_spec(s);s.loader.exec_module(direct)
class PageFlips(unittest.TestCase):
    def test_only_completed_flips_are_counted_and_sequence_gaps_are_visible(self):
        def event(kind,sec,usec,seq):
            data=struct.pack('<IIQIIII',kind,32,1234,sec,usec,seq,37);text=''.join('\\x'+format(b,'02x')for b in data)
            return '123 read(9, "'+text+'", 1024) = 32'
        lines=[event(2,10,0,100),event(1,10,1000,101),event(2,10,20000,102),event(2,10,30000,103)]
        events=direct.flip_events('\n'.join(lines));self.assertEqual(len(events),3)
        stats=direct.flip_summary(events,0);self.assertEqual(stats['missed_vblanks'],1);self.assertAlmostEqual(stats['presented_fps'],2/.03)
        self.assertEqual(direct.flip_events('read(9, "bad", 4) = 4'),[])
        with self.assertRaises(ValueError):direct.flip_summary([],0)
if __name__=='__main__':unittest.main()
