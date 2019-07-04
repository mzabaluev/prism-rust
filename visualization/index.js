const WebSocket = require('ws')

const wss = new WebSocket.Server({ host: '127.0.0.1', port: 9000 })
let visSocket = null
let prismSocket = null

console.log('Waiting for incoming of visualization and prism')
wss.on('connection', ws => {
  if(visSocket==null){
    visSocket = ws
    console.log('Connected to visualization')
  }
  else {
    prismSocket = ws
    console.log('Connected to prism')
  }
  if(prismSocket!==null){
    prismSocket.on('message', message => {
      console.log(message)
      if(visSocket!==null)
        visSocket.send(`${message}`)
    })
  }
})
