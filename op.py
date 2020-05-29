import cv2
import datetime

cam = cv2.VideoCapture(0)
cam.set(cv2.CAP_PROP_FOURCC, cv2.VideoWriter_fourcc('M', 'J', 'P', 'G'))
cam.set(cv2.CAP_PROP_FRAME_WIDTH, 640)
cam.set(cv2.CAP_PROP_FRAME_HEIGHT, 480)
cam.set(cv2.CAP_PROP_FPS, 30)

t = []
for _ in range(32):
    _, mat = cam.read()
    print(mat.shape)
    t.append(datetime.datetime.now())
print('Elapsed FPS:', 30 / (t[31] - t[1]).total_seconds())
