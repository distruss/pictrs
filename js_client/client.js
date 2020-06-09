const fs = require('fs');
const fetch = require('node-fetch')
const FormData = require('form-data');

const imageUploadUrl = `http://localhost:8080/image`;
let formData = new FormData();

formData.append('images[]', fs.createReadStream('../test.png'));

fetch(imageUploadUrl, {
  method: 'POST',
  body: formData,
})
.then(res => res.json())
.then(res => {
  console.log(res);
})
.catch(error => {
  console.error(error);
});
